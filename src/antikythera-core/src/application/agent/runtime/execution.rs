use super::{ToolError, ToolInvokeError, ToolRuntime, Value};
use crate::logging::{AgentLogger, SessionContext};
use futures::stream::{FuturesUnordered, StreamExt};
use std::time::Instant;

pub(crate) struct ToolExecution {
    pub tool: String,
    pub success: bool,
    pub input: Value,
    pub output: Value,
    pub message: Option<String>,
}

impl ToolRuntime {
    pub(crate) async fn execute(
        &self,
        tool_name: &str,
        input: Value,
        ctx: &SessionContext,
    ) -> Result<ToolExecution, ToolError> {
        let log = AgentLogger::from_context(ctx);
        if tool_name.eq_ignore_ascii_case("list_tools") {
            let manifest = self.build_context(None, ctx).await;
            let output = serde_json::to_value(&manifest).unwrap_or(Value::Null);
            log.debug("Agent requested tool catalogue via list_tools");
            let execution = ToolExecution {
                tool: "list_tools".to_string(),
                success: true,
                input,
                output,
                message: Some(format!(
                    "Configured tools available: {} item(s).",
                    manifest.tools.len()
                )),
            };
            log.info(format!(
                "Tool executed | tool={} success={}",
                execution.tool, execution.success
            ));
            return Ok(execution);
        }

        let key = tool_name.to_lowercase();
        let Some(tool) = self.index.get(&key).cloned() else {
            log.warn(format!(
                "Unknown tool requested by agent | requested_tool={}",
                tool_name
            ));
            return Err(ToolError::UnknownTool(tool_name.to_string()));
        };

        let tool_name = tool.name.clone();

        let server_name = match tool.server.as_deref() {
            Some(name) => name,
            None => {
                log.warn(format!(
                    "Tool configured without server binding | tool={}",
                    tool_name
                ));
                return Err(ToolError::UnboundTool(tool_name));
            }
        };

        let arguments = match input.clone() {
            Value::Null => Value::Object(Default::default()),
            other => other,
        };

        log.debug(format!(
            "Dispatching tool via MCP | tool={} server={}",
            tool_name, server_name
        ));
        let start_time = Instant::now();
        match self
            .bridge
            .invoke_tool(server_name, &tool_name, arguments)
            .await
        {
            Ok(result) => {
                let elapsed = start_time.elapsed();
                log.info(format!(
                    "MCP tool execution round-trip completed | latency_ms={} tool={}",
                    elapsed.as_millis(),
                    tool_name
                ));
                let is_error = result
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let message = extract_tool_message(&result);
                let execution = ToolExecution {
                    tool: tool_name,
                    success: !is_error,
                    input,
                    output: result,
                    message,
                };
                log.info(format!(
                    "Tool executed | tool={} success={}",
                    execution.tool, execution.success
                ));
                Ok(execution)
            }
            Err(ToolInvokeError::NotConfigured { .. }) => Err(ToolError::UnboundTool(tool_name)),
            Err(source) => {
                log.warn(format!(
                    "Tool execution failed | tool={} server={} source={}",
                    tool_name, server_name, source
                ));
                Err(ToolError::Execution {
                    tool: tool_name,
                    source,
                })
            }
        }
    }

    /// Apply bounded concurrency backpressure using semaphore.
    ///
    /// Track execution wait and process times individually.
    ///
    pub(crate) async fn execute_parallel(
        &self,
        tools: Vec<(String, Value)>,
        ctx: &SessionContext,
    ) -> Result<Vec<Result<ToolExecution, ToolError>>, ToolError> {
        let mut futures = FuturesUnordered::new();

        for (tool_name, input) in tools {
            let runtime = self.clone();
            let ctx = ctx.clone();

            futures.push(async move {
                let _permit = runtime.execution_semaphore.acquire().await.map_err(|_| {
                    ToolError::Execution {
                        tool: tool_name.clone(),
                        source: ToolInvokeError::NotConfigured {
                            server: "local_agent".into(),
                        },
                    }
                })?;

                runtime.execute(&tool_name, input, &ctx).await
            });
        }

        let mut results = Vec::new();
        while let Some(res) = futures.next().await {
            results.push(res);
        }

        Ok(results)
    }
}

fn extract_tool_message(result: &Value) -> Option<String> {
    if let Some(array) = result.get("content").and_then(Value::as_array) {
        for block in array {
            if block
                .get("type")
                .and_then(Value::as_str)
                .map(|value| value.eq_ignore_ascii_case("text"))
                .unwrap_or(false)
                && let Some(text) = block.get("text").and_then(Value::as_str)
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    if let Some(structured) = result.get("structuredContent").and_then(Value::as_object)
        && let Some(error) = structured.get("error").and_then(Value::as_object)
        && let Some(message) = error.get("message").and_then(Value::as_str)
    {
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}
