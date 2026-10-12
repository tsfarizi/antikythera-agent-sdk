use super::{AgentDirective, AgentError, ToolRuntime, Value};
use crate::logging::{AgentLogger, SessionContext};
use std::time::Instant;

impl ToolRuntime {
    pub fn parse_agent_action(
        &self,
        content: &str,
        ctx: &SessionContext,
    ) -> Result<AgentDirective, AgentError> {
        let log = AgentLogger::from_context(ctx);
        let start_time = Instant::now();
        let result = if let Some(value) = extract_json(content) {
            self.parse_action_value(value, ctx)
        } else {
            Err(AgentError::InvalidResponse(
                "expected JSON object in agent response".into(),
            ))
        };
        let elapsed = start_time.elapsed();
        log.debug(format!(
            "Action parsing completed | latency_us={:?}",
            elapsed.as_micros()
        ));
        result
    }

    /// Unknown action: try to extract a response value using the configurable fallback keys.
    /// Extra fields the model may add are simply ignored — developers can add proper variants to
    /// AgentDirective when they need to handle a new action type explicitly.
    ///
    /// No `action` field: probe the same configurable fallback keys before giving up.
    ///
    fn parse_action_value(
        &self,
        value: Value,
        ctx: &SessionContext,
    ) -> Result<AgentDirective, AgentError> {
        let log = AgentLogger::from_context(ctx);
        match value {
            Value::Object(map) => {
                if let Some(action) = map.get("action").and_then(Value::as_str) {
                    match action {
                        "call_tool" => {
                            let tool =
                                map.get("tool").and_then(Value::as_str).ok_or_else(|| {
                                    AgentError::InvalidResponse(
                                        "call_tool action missing tool field".into(),
                                    )
                                })?;
                            let input = map.get("input").cloned().unwrap_or(Value::Null);
                            Ok(AgentDirective::CallTool {
                                tool: tool.to_string(),
                                input,
                            })
                        }
                        "final" => {
                            let response = map
                                .get("response")
                                .ok_or_else(|| {
                                    AgentError::InvalidResponse(
                                        "final action missing response field".into(),
                                    )
                                })?
                                .clone();

                            Ok(AgentDirective::Final { response })
                        }
                        other => {
                            let response = self
                                .fallback_response_keys
                                .iter()
                                .find_map(|k| map.get(k.as_str()).cloned())
                                .unwrap_or_else(|| Value::Object(map.clone()));
                            log.warn(format!(
                                "Unknown action value — treating as final response | action={}",
                                other
                            ));
                            Ok(AgentDirective::Final { response })
                        }
                    }
                } else {
                    let response = self
                        .fallback_response_keys
                        .iter()
                        .find_map(|k| map.get(k.as_str()).cloned());

                    if let Some(r) = response {
                        log.warn("No action field in agent response — treating as final response");
                        Ok(AgentDirective::Final { response: r })
                    } else {
                        Err(AgentError::InvalidResponse(
                            "missing action field in agent response".into(),
                        ))
                    }
                }
            }
            Value::String(text) => self.parse_agent_action(&text, ctx),
            other => Err(AgentError::InvalidResponse(format!(
                "unsupported response type: {other}"
            ))),
        }
    }
}

fn extract_json(content: &str) -> Option<Value> {
    let trimmed = content.trim();

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }

    if trimmed.starts_with("```") {
        let stripped = trimmed.trim_start_matches("```json");
        let stripped = stripped.trim_start_matches("```JSON");
        let stripped = stripped.trim_start_matches("```");
        if let Some(end) = stripped.rfind("```") {
            let slice = &stripped[..end];
            if let Ok(value) = serde_json::from_str::<Value>(slice.trim()) {
                return Some(value);
            }
        }
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
        && start < end
    {
        let candidate = &trimmed[start..=end];
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return Some(value);
        }
    }

    None
}
