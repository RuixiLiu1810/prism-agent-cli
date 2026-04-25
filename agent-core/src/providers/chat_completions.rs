use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::sync::watch;

use super::{retry_delay_from_headers, AgentTurnOutcome};
use crate::tools::is_document_tool_name;
use crate::{
    autocompact_threshold_for_model, build_agent_instructions_with_work_state,
    compact_chat_messages_with_limit, default_tool_specs,
    document_artifact_miss, document_fallback_used, effective_tool_choice_for_provider, emit_error,
    emit_status, emit_text_delta, ensure_tool_result_pairing, execute_tool_calls,
    extract_text_blocks_only,
    extract_text_segments, extract_tool_result_blocks, extract_tool_use_blocks,
    hidden_chat_message, max_rounds_for_task, provider_display_name, provider_supports_transport,
    push_reasoning_delta, raw_assistant_message, record_document_question_metrics,
    request_has_binary_attachment_context, resolve_turn_profile, sampling_profile_params,
    take_next_sse_frame, to_chat_completions_tool_schema, tool_choice_for_task,
    tool_result_feedback_for_model, tool_result_has_invalid_arguments_error,
    visible_assistant_message, visible_text_message, visible_tool_result_message,
    AgentRuntimeConfig, AgentRuntimeState, AgentToolCall, AgentTurnDescriptor, ConfigProvider,
    EventSink, ToolCallTracker, ToolExecutorFn, TurnBudget, AGENT_CANCELLED_MESSAGE,
    MAX_CONSECUTIVE_COMPACT_FAILURES,
    TOOL_ARGUMENTS_RETRY_HINT,
};

#[derive(Debug, Clone, Default)]
struct ChatCompletionsToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone)]
struct StreamChatOutcome {
    assistant_content: String,
    reasoning_details: Vec<Value>,
    raw_tool_calls: Vec<Value>,
    tool_calls: Vec<AgentToolCall>,
}

pub fn load_runtime_config(
    config_provider: &dyn ConfigProvider,
    project_root: Option<&str>,
) -> Result<AgentRuntimeConfig, String> {
    let config = config_provider.load_agent_runtime(project_root)?;
    if !provider_supports_transport(&config.provider) {
        return Err(format!(
            "{} cannot be handled by chat_completions runtime.",
            config.provider
        ));
    }
    if config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err(format!(
            "{} API key is not configured.",
            provider_display_name(&config.provider)
        ));
    }
    Ok(config)
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

pub fn transcript_to_chat_messages(
    instructions: &str,
    request: &AgentTurnDescriptor,
    history: &[Value],
) -> Vec<Value> {
    let has_raw_chat_entries = history
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("chat_message"));

    let mut messages = vec![json!({
        "role": "system",
        "content": instructions,
    })];

    for item in history {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
        if has_raw_chat_entries {
            if item_type == "chat_message" {
                if let Some(message) = item.get("message") {
                    messages.push(message.clone());
                }
            }
            continue;
        }

        match item_type {
            "assistant" => {
                let content = extract_text_segments(item).join("\n\n");
                let tool_calls = extract_tool_use_blocks(item);
                if content.trim().is_empty() && tool_calls.is_empty() {
                    continue;
                }
                let mut message = json!({
                    "role": "assistant",
                    "content": if content.trim().is_empty() {
                        Value::Null
                    } else {
                        Value::String(content)
                    },
                });
                if !tool_calls.is_empty() {
                    message["tool_calls"] = Value::Array(tool_calls);
                }
                messages.push(message);
            }
            "user" => {
                let content = extract_text_blocks_only(item).join("\n\n");
                if !content.trim().is_empty() {
                    messages.push(json!({
                        "role": "user",
                        "content": content,
                    }));
                }
                messages.extend(extract_tool_result_blocks(item));
            }
            _ => {}
        }
    }

    messages.push(json!({
        "role": "user",
        "content": request.prompt,
    }));

    messages
}

fn preflight_compact_messages(
    sink: &dyn EventSink,
    tab_id: &str,
    model: &str,
    messages: &mut Vec<Value>,
) -> Result<(), String> {
    let threshold = autocompact_threshold_for_model(model);
    let mut compact_failures = 0u32;

    loop {
        let before_tokens = crate::turn_engine::estimate_messages_tokens(messages);
        if before_tokens <= threshold {
            return Ok(());
        }

        emit_status(
            sink,
            tab_id,
            "compacting_context",
            &format!(
                "Context {} tokens exceeds threshold {}, compacting history...",
                before_tokens, threshold
            ),
        );

        let before_len = messages.len();
        compact_chat_messages_with_limit(messages, threshold);
        let after_tokens = crate::turn_engine::estimate_messages_tokens(messages);
        let reduced = after_tokens < before_tokens || messages.len() < before_len;
        if reduced {
            compact_failures = 0;
            continue;
        }

        compact_failures = compact_failures.saturating_add(1);
        if compact_failures >= MAX_CONSECUTIVE_COMPACT_FAILURES {
            return Err(format!(
                "Context is too large ({} tokens) and compaction did not reduce it after {} attempts.",
                after_tokens, MAX_CONSECUTIVE_COMPACT_FAILURES
            ));
        }
    }
}

async fn stream_chat_completions_response_once(
    sink: &dyn EventSink,
    config: &AgentRuntimeConfig,
    request: &AgentTurnDescriptor,
    messages: Vec<Value>,
    mut cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<StreamChatOutcome, String> {
    let api_key = config.api_key.clone().ok_or_else(|| {
        format!(
            "{} API key is not configured.",
            provider_display_name(&config.provider)
        )
    })?;
    let model = request
        .model
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config.model.clone());
    let resolved_profile = resolve_turn_profile(request);
    let requested_tool_choice = tool_choice_for_task(request, &resolved_profile);
    let (effective_tool_choice, _) =
        effective_tool_choice_for_provider(&config.provider, requested_tool_choice);

    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "tools": default_tool_specs()
            .iter()
            .map(|spec| to_chat_completions_tool_schema(spec, &config.provider))
            .collect::<Vec<_>>(),
        "tool_choice": effective_tool_choice,
    });
    if config.provider == "minimax" {
        body["reasoning_split"] = Value::Bool(true);
    }
    if let Some((temperature, top_p, max_tokens)) = sampling_profile_params(
        Some(&resolved_profile.sampling_profile),
        Some(&config.sampling_profiles),
    ) {
        body["temperature"] = json!(temperature);
        body["top_p"] = json!(top_p);
        body["max_tokens"] = json!(max_tokens);
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))?;
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

    const MAX_RETRIES: u32 = 3;
    let mut response = {
        let mut attempt = 0u32;
        loop {
            let resp = client
                .post(&url)
                .bearer_auth(&api_key)
                .header("Accept", "text/event-stream")
                .header("Content-Type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .map_err(|err| {
                    format!(
                        "{} request failed: {}",
                        provider_display_name(&config.provider),
                        err
                    )
                })?;

            if resp.status().is_success() {
                break resp;
            }

            let status = resp.status();
            let retryable = matches!(status.as_u16(), 429 | 503);
            if retryable && attempt < MAX_RETRIES {
                let sleep_dur = retry_delay_from_headers(resp.headers())
                    .unwrap_or_else(|| Duration::from_secs(1u64 << attempt.min(4)));
                let wait_secs = sleep_dur.as_secs().max(1);
                emit_status(
                    sink,
                    &request.tab_id,
                    "retrying",
                    &format!(
                        "Received {} from {}, retrying in {}s (attempt {}/{})...",
                        status.as_u16(),
                        provider_display_name(&config.provider),
                        wait_secs,
                        attempt + 1,
                        MAX_RETRIES
                    ),
                );
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
            let preview = if resp_body.len() > 500 {
                format!("{}...", &resp_body[..500])
            } else {
                resp_body
            };
            return Err(format!(
                "{} request failed with status {}: {}",
                provider_display_name(&config.provider),
                status,
                preview
            ));
        }
    };

    emit_status(
        sink,
        &request.tab_id,
        "streaming",
        &format!("Connected to {}.", provider_display_name(&config.provider)),
    );
    let mut buffer = String::new();
    let mut assistant_content = String::new();
    let mut reasoning_details = Vec::new();
    let mut tool_call_builders: BTreeMap<usize, ChatCompletionsToolCallBuilder> = BTreeMap::new();

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
                        Ok(result) => result.map_err(|err| {
                            format!(
                                "{} streaming read failed: {}",
                                provider_display_name(&config.provider),
                                err
                            )
                        })?,
                        Err(_) => return Err(format!(
                            "{} streaming read timed out after 120s",
                            provider_display_name(&config.provider)
                        )),
                    }
                }
            }
        } else {
            match tokio::time::timeout(CHUNK_TIMEOUT, response.chunk()).await {
                Ok(result) => result.map_err(|err| {
                    format!(
                        "{} streaming read failed: {}",
                        provider_display_name(&config.provider),
                        err
                    )
                })?,
                Err(_) => {
                    return Err(format!(
                        "{} streaming read timed out after 120s",
                        provider_display_name(&config.provider)
                    ));
                }
            }
        };

        let Some(chunk) = next_chunk else {
            break;
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some((_event_name, data)) = take_next_sse_frame(&mut buffer) {
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
                        format!(
                            "Failed to parse {} streaming payload: {}",
                            provider_display_name(&config.provider),
                            err
                        ),
                    );
                    continue;
                }
            };

            let choice = parsed
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .cloned()
                .unwrap_or_else(|| json!({}));

            if let Some(delta) = choice.get("delta") {
                if let Some(content_text) = delta.get("content").and_then(Value::as_str) {
                    let delta_text = crate::merge_stream_fragment(&assistant_content, content_text);
                    assistant_content.push_str(&delta_text);
                    if !delta_text.is_empty() {
                        emit_text_delta(sink, &request.tab_id, &delta_text);
                    }
                }

                push_reasoning_delta(&mut reasoning_details, delta);

                if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for (fallback_index, tool_call) in tool_calls.iter().enumerate() {
                        let index = tool_call
                            .get("index")
                            .and_then(Value::as_u64)
                            .map(|value| value as usize)
                            .unwrap_or(fallback_index);
                        let builder = tool_call_builders.entry(index).or_default();
                        if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
                            builder.id = id.to_string();
                        }
                        if let Some(function) = tool_call.get("function") {
                            if let Some(name) = function.get("name").and_then(Value::as_str) {
                                builder.name = name.to_string();
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(Value::as_str)
                            {
                                builder.arguments = if builder.arguments.is_empty() {
                                    arguments.to_string()
                                } else {
                                    format!(
                                        "{}{}",
                                        builder.arguments,
                                        crate::merge_stream_fragment(&builder.arguments, arguments)
                                    )
                                };
                            }
                        }
                    }
                }
            }

            if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
                if matches!(finish_reason, "stop" | "tool_calls") {
                    emit_status(
                        sink,
                        &request.tab_id,
                        "completed",
                        &format!(
                            "{} response completed.",
                            provider_display_name(&config.provider)
                        ),
                    );
                }
            }
        }
    }

    let tool_calls = tool_call_builders
        .into_values()
        .map(|builder| AgentToolCall {
            tool_name: builder.name.clone(),
            call_id: builder.id.clone(),
            arguments: if builder.arguments.trim().is_empty() {
                "{}".to_string()
            } else {
                builder.arguments.clone()
            },
        })
        .collect::<Vec<_>>();

    let raw_tool_calls = tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.call_id,
                "type": "function",
                "function": {
                    "name": call.tool_name,
                    "arguments": call.arguments,
                }
            })
        })
        .collect::<Vec<_>>();

    Ok(StreamChatOutcome {
        assistant_content,
        reasoning_details,
        raw_tool_calls,
        tool_calls,
    })
}

pub async fn run_turn_loop(
    sink: &dyn EventSink,
    config_provider: &dyn ConfigProvider,
    runtime_state: &AgentRuntimeState,
    request: &AgentTurnDescriptor,
    history: &[Value],
    tool_executor: ToolExecutorFn,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<AgentTurnOutcome, String> {
    let app_config_dir = config_provider.app_config_dir()?;
    runtime_state.ensure_storage_at(app_config_dir).await?;

    let runtime = load_runtime_config(config_provider, Some(&request.project_path))?;
    let mut transcript_messages = vec![
        visible_text_message("user", &request.prompt),
        hidden_chat_message(json!({
            "role": "user",
            "content": request.prompt,
        })),
    ];
    let resolved_profile = resolve_turn_profile(request);
    let mut instructions =
        agent_instructions_for_request(runtime_state, request, Some(&runtime)).await;
    let requested_tool_choice = tool_choice_for_task(request, &resolved_profile);
    let (_, downgraded_tool_choice) =
        effective_tool_choice_for_provider(&runtime.provider, requested_tool_choice);
    if downgraded_tool_choice {
        instructions.push_str(
            "\n[Tool-calling fallback]\n\
            This provider may ignore tool_choice='required'. You MUST call at least one appropriate tool before finalizing the answer for this turn.\n",
        );
    }
    let mut next_messages = transcript_to_chat_messages(&instructions, request, history);
    ensure_tool_result_pairing(&mut next_messages);
    preflight_compact_messages(sink, &request.tab_id, &runtime.model, &mut next_messages)?;
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
            Some(&runtime.sampling_profiles),
        )
        .map(|(_, _, max_tokens)| max_tokens),
        cancel_rx.clone(),
    );
    let mut tracker = ToolCallTracker::new(budget.max_rounds);

    for round_idx in 0..budget.max_rounds {
        preflight_compact_messages(sink, &request.tab_id, &runtime.model, &mut next_messages)?;
        tracker.current_round = round_idx;
        budget.ensure_round_available(round_idx)?;
        let outcome = stream_chat_completions_response_once(
            sink,
            &runtime,
            request,
            next_messages.clone(),
            budget.clone_abort_rx(),
        )
        .await?;
        budget.record_output_text(&outcome.assistant_content)?;
        let raw_assistant = raw_assistant_message(
            &outcome.assistant_content,
            &outcome.reasoning_details,
            &outcome.raw_tool_calls,
        );

        transcript_messages.push(visible_assistant_message(
            &outcome.assistant_content,
            &outcome.tool_calls,
        ));
        transcript_messages.push(hidden_chat_message(raw_assistant.clone()));

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
                response_id: None,
                messages: transcript_messages,
                suspended: false,
            });
        }

        let round_doc_calls = outcome
            .tool_calls
            .iter()
            .filter(|call| is_document_tool_name(&call.tool_name))
            .count() as u32;
        if round_doc_calls > 0 {
            doc_tool_rounds = doc_tool_rounds.saturating_add(1);
            doc_tool_calls = doc_tool_calls.saturating_add(round_doc_calls);
        }

        let mut tool_results_messages = vec![raw_assistant];
        let mut invalid_tool_arguments_detected = false;

        for call in &outcome.tool_calls {
            tracker.record_call(&call.tool_name, &call.arguments);
        }

        let executed_calls = execute_tool_calls(
            sink,
            runtime_state,
            request,
            outcome.tool_calls,
            budget.clone_abort_rx(),
            tool_executor.clone(),
        )
        .await;
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
            tool_results_messages.push(json!({
                "role": "tool",
                "tool_call_id": result.call_id,
                "content": feedback,
            }));
            transcript_messages.push(hidden_chat_message(
                tool_results_messages
                    .last()
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            ));
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
                response_id: None,
                messages: transcript_messages,
                suspended: true,
            });
        }

        next_messages.extend(tool_results_messages);
        ensure_tool_result_pairing(&mut next_messages);
        preflight_compact_messages(sink, &request.tab_id, &runtime.model, &mut next_messages)?;

        if let Some(injection) = tracker.build_injection(round_idx) {
            next_messages.push(json!({
                "role": "system",
                "content": injection,
            }));
        }

        if invalid_tool_arguments_detected {
            next_messages.push(json!({
                "role": "system",
                "content": TOOL_ARGUMENTS_RETRY_HINT,
            }));
            emit_status(
                sink,
                &request.tab_id,
                "tool_retry_hint",
                "Tool arguments were invalid. Retrying with strict JSON argument guidance.",
            );
        }
        emit_status(
            sink,
            &request.tab_id,
            "responding_after_tools",
            &format!(
                "Tool results sent back to {}. Continuing...",
                provider_display_name(&runtime.provider)
            ),
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
        "{} tool loop exceeded {} rounds; aborting to avoid an infinite agent loop.",
        provider_display_name(&runtime.provider),
        max_rounds_for_task(&resolved_profile)
    ))
}

#[cfg(test)]
mod tests {
    use super::{load_runtime_config, transcript_to_chat_messages};
    use crate::{
        build_agent_instructions_with_work_state, effective_tool_choice_for_provider,
        merge_stream_fragment, provider_display_name, provider_supports_transport,
        AgentRuntimeConfig, AgentTurnDescriptor, StaticConfigProvider,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn make_request(prompt: &str) -> AgentTurnDescriptor {
        AgentTurnDescriptor {
            project_path: "/tmp/project".to_string(),
            prompt: prompt.to_string(),
            tab_id: "tab-test".to_string(),
            model: None,
            local_session_id: None,
            previous_response_id: None,
            turn_profile: None,
        }
    }

    #[test]
    fn chat_completions_runtime_accepts_only_minimax_or_deepseek() {
        let mut runtime = AgentRuntimeConfig::default_local_agent();
        runtime.provider = "openai".to_string();
        runtime.api_key = Some("test-key".to_string());

        let config_provider = StaticConfigProvider {
            config: runtime,
            config_dir: PathBuf::from("/tmp/agent-core-chat-completions"),
        };
        let err = load_runtime_config(&config_provider, Some("/tmp/project"))
            .expect_err("non-chat-completions provider should fail");

        assert!(err.contains("openai"));
    }

    #[test]
    fn converts_text_transcript_into_chat_messages() {
        let history = vec![
            json!({
                "type": "user",
                "message": {
                    "content": [{ "type": "text", "text": "hello" }]
                }
            }),
            json!({
                "type": "assistant",
                "message": {
                    "content": [{ "type": "text", "text": "hi there" }]
                }
            }),
        ];

        let request = make_request("continue");
        let instructions = build_agent_instructions_with_work_state(&request, None, None, None);
        let messages = transcript_to_chat_messages(&instructions, &request, &history);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "system");
        assert!(messages[0]["content"]
            .as_str()
            .unwrap_or_default()
            .contains("execution-oriented agent"));
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "hello");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], "hi there");
        assert_eq!(messages[3]["role"], "user");
        assert_eq!(messages[3]["content"], "continue");
    }

    #[test]
    fn uses_raw_chat_messages_when_present() {
        let history = vec![
            json!({
                "type": "user",
                "message": {
                    "content": [{ "type": "text", "text": "display only" }]
                }
            }),
            json!({
                "type": "chat_message",
                "message": {
                    "role": "user",
                    "content": "real prompt"
                }
            }),
        ];

        let request = make_request("continue");
        let instructions = build_agent_instructions_with_work_state(&request, None, None, None);
        let messages = transcript_to_chat_messages(&instructions, &request, &history);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert!(messages[0]["content"]
            .as_str()
            .unwrap_or_default()
            .contains("execution-oriented agent"));
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "real prompt");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "continue");
    }

    #[test]
    fn merge_fragment_handles_cumulative_and_incremental_chunks() {
        assert_eq!(merge_stream_fragment("", "abc"), "abc");
        assert_eq!(merge_stream_fragment("abc", "abcdef"), "def");
        assert_eq!(merge_stream_fragment("abc", "def"), "def");
    }

    #[test]
    fn provider_transport_matrix_includes_deepseek() {
        assert!(provider_supports_transport("minimax"));
        assert!(provider_supports_transport("deepseek"));
        assert!(!provider_supports_transport("openai"));
        assert_eq!(
            provider_display_name("deepseek"),
            "DeepSeek Chat Completions"
        );
    }

    #[test]
    fn deepseek_downgrades_required_tool_choice_to_auto() {
        let (choice, downgraded) = effective_tool_choice_for_provider("deepseek", "required");
        assert_eq!(choice, "auto");
        assert!(downgraded);

        let (choice_minimax, downgraded_minimax) =
            effective_tool_choice_for_provider("minimax", "required");
        assert_eq!(choice_minimax, "required");
        assert!(!downgraded_minimax);
    }

    #[test]
    fn reconstructs_tool_context_from_visible_transcript_when_raw_history_is_missing() {
        let history = vec![
            json!({
                "type": "assistant",
                "message": {
                    "content": [
                        { "type": "text", "text": "I'll patch the file." },
                        {
                            "type": "tool_use",
                            "id": "call_1",
                            "name": "apply_text_patch",
                            "input": {
                                "path": "main.tex",
                                "expected_old_text": "old",
                                "new_text": "new"
                            }
                        }
                    ]
                }
            }),
            json!({
                "type": "user",
                "message": {
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "call_1",
                            "content": "Edit applied successfully to main.tex.",
                            "is_error": false
                        }
                    ]
                }
            }),
        ];

        let request = make_request("continue");
        let instructions = build_agent_instructions_with_work_state(&request, None, None, None);
        let messages = transcript_to_chat_messages(&instructions, &request, &history);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
    }
}
