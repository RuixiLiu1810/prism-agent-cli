use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReviewArtifact {
    pub artifact_type: String,
    pub tool_name: String,
    pub approval_tool_name: String,
    pub target_path: String,
    pub absolute_path: String,
    pub old_content: String,
    pub new_content: String,
    pub selection_range: Option<String>,
    pub summary: Option<String>,
    pub written: bool,
}

impl AgentReviewArtifact {
    pub fn to_content_value(
        &self,
        approval_required: bool,
        reason: Option<&str>,
        input: Option<Value>,
        extra: Value,
    ) -> Value {
        let mut payload = json!({
            "toolName": self.tool_name,
            "approvalToolName": self.approval_tool_name,
            "path": self.target_path,
            "absolutePath": self.absolute_path,
            "oldContent": self.old_content,
            "newContent": self.new_content,
            "written": self.written,
            "reviewArtifact": true,
            "reviewArtifactPayload": self,
        });

        if let Some(object) = payload.as_object_mut() {
            if approval_required {
                object.insert("approvalRequired".to_string(), json!(true));
            }
            if let Some(reason) = reason {
                object.insert("reason".to_string(), json!(reason));
            }
            if let Some(input) = input {
                object.insert("input".to_string(), input);
            }
            if let Some(extra_obj) = extra.as_object() {
                merge_extra(object, extra_obj);
            }
        }

        payload
    }
}

fn merge_extra(target: &mut Map<String, Value>, extra: &Map<String, Value>) {
    for (key, value) in extra {
        target.insert(key.clone(), value.clone());
    }
}
