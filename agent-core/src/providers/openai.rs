use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::sync::watch;

use super::AgentTurnOutcome;
use crate::tools::is_document_tool_name;
use crate::{
    build_agent_instructions_with_work_state, default_tool_specs, emit_error, emit_status,
    emit_text_delta, execute_tool_calls, max_rounds_for_task, record_document_question_metrics,
    request_has_binary_attachment_context, resolve_turn_profile, sampling_profile_params,
    should_surface_assistant_text, to_openai_tool_schema, tool_choice_for_task,
    tool_result_feedback_for_model, tool_result_has_invalid_arguments_error, AgentRuntimeConfig,
    AgentRuntimeState, AgentSamplingProfilesConfig, AgentToolCall, AgentTurnDescriptor,
    ConfigProvider, EventSink, ToolCallTracker, ToolExecutorFn, TurnBudget,
    AGENT_CANCELLED_MESSAGE, TOOL_ARGUMENTS_RETRY_HINT,
};
use crate::{document_artifact_miss, document_fallback_used};
use crate::{extract_function_call_item, extract_response_id, take_next_sse_frame};
use crate::{visible_text_message, visible_tool_result_message};

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
    pub source: String,
    pub sampling_profiles: Option<AgentSamplingProfilesConfig>,
}

#[derive(Debug, Clone)]
struct StreamResponseOutcome {
    response_id: Option<String>,
    tool_calls: Vec<AgentToolCall>,
    assistant_text: String,
}

pub fn runtime_config_from_agent_runtime(
    runtime: AgentRuntimeConfig,
    env_api_key: Option<String>,
) -> Result<OpenAiConfig, String> {
    if runtime.provider != "openai" {
        return Err(format!(
            "OpenAI Responses adapter cannot handle provider `{}`.",
            runtime.provider
        ));
    }

    let settings_api_key = runtime
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let env_api_key = env_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let (api_key, source) = if let Some(api_key) = settings_api_key {
        (api_key, "settings")
    } else if let Some(api_key) = env_api_key {
        (api_key, "env")
    } else {
        return Err("OPENAI_API_KEY is not set".to_string());
    };

    Ok(OpenAiConfig {
        api_key,
        base_url: runtime.base_url.trim_end_matches('/').to_string(),
        default_model: runtime.model,
        source: source.to_string(),
        sampling_profiles: Some(runtime.sampling_profiles),
    })
}

pub fn load_runtime_config(
    config_provider: &dyn ConfigProvider,
    project_root: Option<&str>,
) -> Result<OpenAiConfig, String> {
    let runtime = config_provider.load_agent_runtime(project_root)?;
    runtime_config_from_agent_runtime(runtime, std::env::var("OPENAI_API_KEY").ok())
}

async fn agent_instructions_for_request(
    state: &AgentRuntimeState,
    request: &AgentTurnDescriptor,
    runtime_config: Option<&AgentRuntimeConfig>,
) -> String {
    let work_state = state
        .work_state_for_prompt(&request.tab_id, request.local_session_id.as_deref())
        .await;
    let memory_context = state.build_memory_context().await;
    let mem_ref = if memory_context.is_empty() {
        None
    } else {
        Some(memory_context.as_str())
    };
    build_agent_instructions_with_work_state(request, Some(&work_state), runtime_config, mem_ref)
}

async fn stream_response_once(
    sink: &dyn EventSink,
    config: &OpenAiConfig,
    request: &AgentTurnDescriptor,
    instructions: &str,
    input: Value,
    previous_response_id: Option<String>,
    mut cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<StreamResponseOutcome, String> {
    let model = request
        .model
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config.default_model.clone());
    let resolved_profile = resolve_turn_profile(request);

    let mut body = json!({
        "model": model,
        "input": input,
        "instructions": instructions,
        "stream": true,
        "parallel_tool_calls": false,
        "tool_choice": tool_choice_for_task(request, &resolved_profile),
        "tools": default_tool_specs()
            .iter()
            .map(to_openai_tool_schema)
            .collect::<Vec<_>>(),
    });

    if let Some((temperature, top_p, max_output_tokens)) = sampling_profile_params(
        Some(&resolved_profile.sampling_profile),
        config.sampling_profiles.as_ref(),
    ) {
        body["temperature"] = json!(temperature);
        body["top_p"] = json!(top_p);
        body["max_output_tokens"] = json!(max_output_tokens);
    }

    if let Some(previous_response_id) = previous_response_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        body["previous_response_id"] = json!(previous_response_id);
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;
    let url = format!("{}/responses", config.base_url);

    const MAX_RETRIES: u32 = 3;
    let mut response = {
        let mut attempt = 0u32;
        loop {
            let resp = client
                .post(&url)
                .bearer_auth(&config.api_key)
                .header("Accept", "text/event-stream")
                .header("Content-Type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|err| format!("OpenAI request failed: {}", err))?;

            if resp.status().is_success() {
                break resp;
            }

            let status = resp.status();
            let retryable = matches!(status.as_u16(), 429 | 503);
            if retryable && attempt < MAX_RETRIES {
                let backoff_secs = 1u64 << attempt.min(4);
                emit_status(
                    sink,
                    &request.tab_id,
                    "retrying",
                    &format!(
                        "Received {} from OpenAI, retrying in {}s (attempt {}/{})...",
                        status.as_u16(),
                        backoff_secs,
                        attempt + 1,
                        MAX_RETRIES
                    ),
                );
                let sleep_dur = Duration::from_secs(backoff_secs);
                if let Some(rx) = cancel_rx.as_mut() {
                    tokio::select! {
                        _ = tokio::time::sleep(sleep_dur) => {}
                        changed = rx.changed() => {
                            if changed.is_err() || *rx.borrow() {
                                return Err(AGENT_CANCELLED_MESSAGE.to_string());
                            }
                        }
                    }
                } else {
                    tokio::time::sleep(sleep_dur).await;
                }
                attempt += 1;
                continue;
            }

            let resp_body = resp.text().await.unwrap_or_default();
            let preview = if resp_body.len() > 400 {
                format!("{}...", &resp_body[..400])
            } else {
                resp_body
            };
            return Err(format!(
                "OpenAI Responses request failed with status {}: {}",
                status, preview
            ));
        }
    };

    emit_status(
        sink,
        &request.tab_id,
        "streaming",
        "Connected to OpenAI Responses API.",
    );
    let mut buffer = String::new();
    let mut final_response_id = None;
    let mut tool_calls = Vec::new();
    let mut assistant_text = String::new();

    const CHUNK_TIMEOUT: Duration = Duration::from_secs(120);

    loop {
        let next_chunk = if let Some(cancel_rx) = cancel_rx.as_mut() {
            tokio::select! {
                changed = cancel_rx.changed() => {
                    match changed {
                        Ok(_) if *cancel_rx.borrow() => return Err(AGENT_CANCELLED_MESSAGE.to_string()),
                        Ok(_) => continue,
                        Err(_) => return Err(AGENT_CANCELLED_MESSAGE.to_string()),
                    }
                }
                chunk = tokio::time::timeout(CHUNK_TIMEOUT, response.chunk()) => {
                    match chunk {
                        Ok(result) => result.map_err(|err| format!("OpenAI streaming read failed: {}", err))?,
                        Err(_) => return Err("OpenAI streaming read timed out after 120s".to_string()),
                    }
                }
            }
        } else {
            match tokio::time::timeout(CHUNK_TIMEOUT, response.chunk()).await {
                Ok(result) => {
                    result.map_err(|err| format!("OpenAI streaming read failed: {}", err))?
                }
                Err(_) => return Err("OpenAI streaming read timed out after 120s".to_string()),
            }
        };

        let Some(chunk) = next_chunk else {
            break;
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some((event_name, data)) = take_next_sse_frame(&mut buffer) {
            if data == "[DONE]" {
                continue;
            }

            let parsed: Value = match serde_json::from_str(&data) {
                Ok(value) => value,
                Err(err) => {
                    emit_error(
                        sink,
                        &request.tab_id,
                        "agent_stream_parse_error",
                        format!("Failed to parse streaming event {}: {}", event_name, err),
                    );
                    continue;
                }
            };

            if final_response_id.is_none() {
                final_response_id = extract_response_id(&parsed);
            }

            match event_name.as_str() {
                "response.created" => {
                    emit_status(sink, &request.tab_id, "created", "OpenAI response created.");
                }
                "response.output_text.delta" => {
                    if let Some(delta) = parsed.get("delta").and_then(Value::as_str) {
                        assistant_text.push_str(delta);
                        emit_text_delta(sink, &request.tab_id, delta);
                    }
                }
                "response.completed" => {
                    final_response_id = extract_response_id(&parsed).or(final_response_id);
                    if let Some(output_items) =
                        parsed.pointer("/response/output").and_then(Value::as_array)
                    {
                        for item in output_items {
                            if let Some(call) = extract_function_call_item(item) {
                                tool_calls.push(call);
                            }
                        }
                    }
                    emit_status(
                        sink,
                        &request.tab_id,
                        "completed",
                        "OpenAI response completed.",
                    );
                }
                "response.failed" | "response.incomplete" | "error" => {
                    let message = parsed
                        .get("message")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            parsed
                                .get("error")
                                .and_then(|error| error.get("message"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or("OpenAI returned an error event.");
                    emit_error(
                        sink,
                        &request.tab_id,
                        "agent_provider_error",
                        message.to_string(),
                    );
                }
                "response.function_call_arguments.done" => {
                    if let Some(item) = parsed.get("item").and_then(extract_function_call_item) {
                        tool_calls.push(item);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(StreamResponseOutcome {
        response_id: final_response_id,
        tool_calls,
        assistant_text,
    })
}

pub async fn run_turn_loop(
    sink: &dyn EventSink,
    config_provider: &dyn ConfigProvider,
    runtime_state: &AgentRuntimeState,
    request: &AgentTurnDescriptor,
    tool_executor: ToolExecutorFn,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<AgentTurnOutcome, String> {
    let app_config_dir = config_provider.app_config_dir()?;
    runtime_state.ensure_storage_at(app_config_dir).await?;

    let config = load_runtime_config(config_provider, Some(&request.project_path))?;
    let runtime_settings = config_provider.load_agent_runtime(Some(&request.project_path))?;
    let resolved_profile = resolve_turn_profile(request);

    let mut previous_response_id = request.previous_response_id.clone();
    let mut next_input = json!([
        {
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": request.prompt
                }
            ]
        }
    ]);
    let mut latest_response_id = None;
    let mut transcript_messages = vec![visible_text_message("user", &request.prompt)];
    let mut instructions =
        agent_instructions_for_request(runtime_state, request, Some(&runtime_settings)).await;
    let turn_started_at = Instant::now();
    let mut doc_tool_rounds = 0u32;
    let mut doc_tool_calls = 0u32;
    let mut artifact_miss_count = 0u32;
    let mut fallback_count = 0u32;
    let is_document_question = request_has_binary_attachment_context(request);
    let mut budget = TurnBudget::new(
        max_rounds_for_task(&resolved_profile),
        sampling_profile_params(
            Some(&resolved_profile.sampling_profile),
            config.sampling_profiles.as_ref(),
        )
        .map(|(_, _, max_output_tokens)| max_output_tokens),
        cancel_rx.clone(),
    );
    let mut tracker = ToolCallTracker::new(budget.max_rounds);

    for round_idx in 0..budget.max_rounds {
        tracker.current_round = round_idx;
        budget.ensure_round_available(round_idx)?;
        let outcome = stream_response_once(
            sink,
            &config,
            request,
            &instructions,
            next_input,
            previous_response_id.clone(),
            budget.clone_abort_rx(),
        )
        .await?;
        budget.record_output_text(&outcome.assistant_text)?;

        latest_response_id = outcome.response_id.clone().or(latest_response_id);

        if should_surface_assistant_text(&outcome.assistant_text, &outcome.tool_calls) {
            transcript_messages.push(visible_text_message("assistant", &outcome.assistant_text));
        }

        if outcome.tool_calls.is_empty() {
            if is_document_question || doc_tool_calls > 0 {
                record_document_question_metrics(
                    runtime_state,
                    request,
                    "completed",
                    doc_tool_rounds,
                    doc_tool_calls,
                    artifact_miss_count,
                    fallback_count,
                    turn_started_at.elapsed(),
                )
                .await;
            }
            return Ok(AgentTurnOutcome {
                response_id: latest_response_id,
                messages: transcript_messages,
                suspended: false,
            });
        }

        let mut seen_call_ids = HashSet::new();
        let deduped_tool_calls: Vec<AgentToolCall> = outcome
            .tool_calls
            .into_iter()
            .filter(|call| seen_call_ids.insert(call.call_id.clone()))
            .collect();
        let round_doc_calls = deduped_tool_calls
            .iter()
            .filter(|call| is_document_tool_name(&call.tool_name))
            .count() as u32;
        if round_doc_calls > 0 {
            doc_tool_rounds = doc_tool_rounds.saturating_add(1);
            doc_tool_calls = doc_tool_calls.saturating_add(round_doc_calls);
        }

        for call in &deduped_tool_calls {
            tracker.record_call(&call.tool_name, &call.arguments);
        }

        let executed_calls = execute_tool_calls(
            sink,
            runtime_state,
            request,
            deduped_tool_calls,
            budget.clone_abort_rx(),
            tool_executor.clone(),
        )
        .await;

        let mut tool_outputs = Vec::new();
        let mut invalid_tool_arguments_detected = false;
        for executed in &executed_calls.executed {
            let result = executed.result.clone();
            if result.content.get("error").and_then(Value::as_str) == Some(AGENT_CANCELLED_MESSAGE)
            {
                if is_document_question || doc_tool_calls > 0 {
                    record_document_question_metrics(
                        runtime_state,
                        request,
                        "cancelled",
                        doc_tool_rounds,
                        doc_tool_calls,
                        artifact_miss_count,
                        fallback_count,
                        turn_started_at.elapsed(),
                    )
                    .await;
                }
                return Err(AGENT_CANCELLED_MESSAGE.to_string());
            }
            if is_document_tool_name(&result.tool_name) {
                if document_artifact_miss(&result) {
                    artifact_miss_count = artifact_miss_count.saturating_add(1);
                }
                if document_fallback_used(&result) {
                    fallback_count = fallback_count.saturating_add(1);
                }
            }
            if tool_result_has_invalid_arguments_error(&result) {
                invalid_tool_arguments_detected = true;
            }
            let feedback = tool_result_feedback_for_model(&result);
            budget.record_output_text(&feedback)?;
            transcript_messages.push(visible_tool_result_message(
                &result.call_id,
                &result.preview,
                result.is_error,
            ));

            tool_outputs.push(json!({
                "type": "function_call_output",
                "call_id": result.call_id,
                "output": feedback,
            }));
        }

        if executed_calls.suspended {
            if is_document_question || doc_tool_calls > 0 {
                record_document_question_metrics(
                    runtime_state,
                    request,
                    "suspended",
                    doc_tool_rounds,
                    doc_tool_calls,
                    artifact_miss_count,
                    fallback_count,
                    turn_started_at.elapsed(),
                )
                .await;
            }
            return Ok(AgentTurnOutcome {
                response_id: latest_response_id,
                messages: transcript_messages,
                suspended: true,
            });
        }

        previous_response_id = outcome.response_id.or(previous_response_id);
        if invalid_tool_arguments_detected
            && !instructions.contains("[Tool argument recovery rule]")
        {
            instructions.push('\n');
            instructions.push_str(TOOL_ARGUMENTS_RETRY_HINT);
            instructions.push('\n');
            emit_status(
                sink,
                &request.tab_id,
                "tool_retry_hint",
                "Tool arguments were invalid. Retrying with strict JSON argument guidance.",
            );
        }

        if let Some(injection) = tracker.build_injection(round_idx) {
            tool_outputs.push(json!({
                "type": "message",
                "role": "user",
                "content": injection,
            }));
        }

        next_input = Value::Array(tool_outputs);
        emit_status(
            sink,
            &request.tab_id,
            "responding_after_tools",
            "Tool results sent back to the model. Continuing...",
        );
    }

    if is_document_question || doc_tool_calls > 0 {
        record_document_question_metrics(
            runtime_state,
            request,
            "round_limit_exceeded",
            doc_tool_rounds,
            doc_tool_calls,
            artifact_miss_count,
            fallback_count,
            turn_started_at.elapsed(),
        )
        .await;
    }

    Err(format!(
        "Tool loop exceeded {} rounds; aborting to avoid an infinite agent loop.",
        max_rounds_for_task(&resolved_profile)
    ))
}

#[cfg(test)]
mod tests {
    use super::runtime_config_from_agent_runtime;
    use crate::AgentRuntimeConfig;

    #[test]
    fn openai_runtime_rejects_non_openai_provider() {
        let mut runtime = AgentRuntimeConfig::default_local_agent();
        runtime.provider = "deepseek".to_string();

        let err = runtime_config_from_agent_runtime(runtime, Some("env-key".to_string()))
            .expect_err("non-openai provider should fail");

        assert!(err.contains("deepseek"));
    }

    #[test]
    fn merge_runtime_with_env_key_preserves_runtime_model_base_url_and_sampling() {
        let runtime = AgentRuntimeConfig::default_local_agent();
        let expected_model = runtime.model.clone();
        let expected_base_url = runtime.base_url.clone();
        let expected_temp = runtime.sampling_profiles.edit_stable.temperature;

        let merged = runtime_config_from_agent_runtime(runtime, Some("env-key".to_string()))
            .expect("env key should be accepted");

        assert_eq!(merged.api_key, "env-key");
        assert_eq!(merged.default_model, expected_model);
        assert_eq!(merged.base_url, expected_base_url);
        assert_eq!(
            merged
                .sampling_profiles
                .as_ref()
                .expect("sampling profiles should be preserved")
                .edit_stable
                .temperature,
            expected_temp
        );
        assert_eq!(merged.source, "env");
    }

    #[test]
    fn merge_runtime_with_api_key_prefers_settings_key_when_present() {
        let mut runtime = AgentRuntimeConfig::default_local_agent();
        runtime.api_key = Some("settings-key".to_string());

        let merged = runtime_config_from_agent_runtime(runtime, Some("env-key".to_string()))
            .expect("settings key should win");

        assert_eq!(merged.api_key, "settings-key");
        assert_eq!(merged.source, "settings");
    }

    #[test]
    fn merge_runtime_with_api_key_errors_when_no_key_is_available() {
        let runtime = AgentRuntimeConfig::default_local_agent();
        let err =
            runtime_config_from_agent_runtime(runtime, None).expect_err("missing key should fail");
        assert!(err.contains("OPENAI_API_KEY"));
    }
}
