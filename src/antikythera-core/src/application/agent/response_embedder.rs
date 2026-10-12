use super::models::AgentStep;
use serde_json::Value;

pub struct ResponseEmbedder;

impl ResponseEmbedder {
    pub fn embed_tool_results_sync(response: Value, steps: &[AgentStep]) -> Value {
        match response {
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (key, value) in obj {
                    let processed_value = Self::embed_tool_results_sync(value, steps);
                    new_obj.insert(key, processed_value);
                }
                Value::Object(new_obj)
            }
            Value::Array(arr) => {
                let new_arr: Vec<Value> = arr
                    .into_iter()
                    .map(|item| Self::embed_tool_results_sync(item, steps))
                    .collect();
                Value::Array(new_arr)
            }
            Value::String(s) => {
                if (s.starts_with("step_") || s.starts_with("result_"))
                    && !s.contains(' ')
                    && let Some(step_idx) = s
                        .strip_prefix("step_")
                        .or_else(|| s.strip_prefix("result_"))
                    && let Ok(idx) = step_idx.parse::<usize>()
                {
                    if let Some(step) = steps.get(idx) {
                        return Self::extract_result_data(&step.output);
                    } else if idx > 0
                        && let Some(step) = steps.get(idx - 1)
                    {
                        return Self::extract_result_data(&step.output);
                    }
                }

                let mut result_str = s.clone();
                let mut modified = false;

                for i in (0..=steps.len()).rev() {
                    let step_pattern = format!("step_{}", i);
                    let result_pattern = format!("result_{}", i);

                    if result_str.contains(&step_pattern) || result_str.contains(&result_pattern) {
                        let step_to_use = if i < steps.len() {
                            Some(&steps[i])
                        } else if i > 0 && (i - 1) < steps.len() {
                            Some(&steps[i - 1])
                        } else {
                            None
                        };

                        if let Some(step) = step_to_use {
                            let data = Self::extract_result_data(&step.output);
                            let replacement = match &data {
                                Value::String(inner_s) => inner_s.clone(),
                                _ => serde_json::to_string(&data)
                                    .unwrap_or_else(|_| "null".to_string()),
                            };

                            result_str = result_str.replace(&step_pattern, &replacement);
                            result_str = result_str.replace(&result_pattern, &replacement);
                            modified = true;
                        }
                    }
                }

                if modified {
                    Value::String(result_str)
                } else {
                    Value::String(s)
                }
            }
            _ => response,
        }
    }

    /// Extract a JSON-RPC result or the single text block of an MCP response.
    ///
    /// A `result` field takes precedence over all other representations.
    /// JSON-RPC objects without a result retain application fields after
    /// removing protocol metadata; metadata-only objects remain unchanged.
    /// Text blocks containing JSON are decoded, and other text remains a string.
    pub fn extract_result_data(output: &Value) -> Value {
        if let Some(obj) = output.as_object() {
            if let Some(result) = obj.get("result") {
                return result.clone();
            }
            if obj.contains_key("jsonrpc") || obj.contains_key("id") {
                let filtered_obj: serde_json::Map<String, Value> = obj
                    .iter()
                    .filter(|(k, _)| !["jsonrpc", "id", "error"].contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                if filtered_obj.is_empty() {
                    return output.clone();
                } else {
                    return Value::Object(filtered_obj);
                }
            }

            if let Some(content_arr) = obj.get("content").and_then(|c| c.as_array())
                && content_arr.len() == 1
                && let Some(block) = content_arr[0].as_object()
                && block.get("type").and_then(|t| t.as_str()) == Some("text")
                && let Some(text) = block.get("text").and_then(|t| t.as_str())
            {
                if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                    return parsed;
                }
                return Value::String(text.to_string());
            }
        }

        output.clone()
    }
}

#[cfg(test)]
#[path = "../../../../../tests/rust/unit/antikythera-core/application/agent/response_embedder.tests.rs"]
mod tests;
