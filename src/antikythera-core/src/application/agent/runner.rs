use super::directive::AgentDirective;
use super::errors::AgentError;
use super::events::DomainEvent;
use super::models::{AgentOptions, AgentOutcome, AgentStep};
use super::response_embedder::ResponseEmbedder;
use super::runtime::ToolRuntime;
use super::tool_result_parser::ToolResultParser;
use crate::application::client::{ChatRequest, McpClient};
use crate::application::model_provider::ModelProvider;
use crate::application::ports::security::RateLimiter as RateLimiterTrait;
use crate::logging::{AgentLogger, SessionContext};
use serde_json::{Value, json};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use sysinfo::System;

pub struct Agent<P: ModelProvider> {
    client: Arc<McpClient<P>>,
    runtime: ToolRuntime,
    rate_limiter: Option<Arc<dyn RateLimiterTrait>>,
}

impl<P: ModelProvider> Agent<P> {
    pub fn new(client: Arc<McpClient<P>>) -> Self {
        let tools = client.tools().to_vec();
        let bridge = client.server_bridge();
        let fallback_keys: Vec<String> = client
            .prompts()
            .fallback_response_keys()
            .into_iter()
            .map(str::to_string)
            .collect();
        Self {
            client,
            runtime: ToolRuntime::new(tools, bridge).with_fallback_keys(fallback_keys),
            rate_limiter: None,
        }
    }

    /// Attach a rate limiter to this agent. When set, every LLM call
    /// will be checked against the rate limit before execution.
    pub fn with_rate_limiter(mut self, limiter: Arc<dyn RateLimiterTrait>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Rate limit check before making LLM calls.
    ///
    /// Log the prompt being sent for IO trace.
    ///
    /// Agent composes its own complete system prompt.
    ///
    /// Rate limit check before each LLM call.
    ///
    /// Log the LLM response for IO trace.
    ///
    /// Parses agent action with retry logic for malformed JSON.
    ///
    /// Use configurable tool result instruction Use configurable tool result instruction.
    ///
    pub async fn run(
        &self,
        prompt: String,
        mut options: AgentOptions,
    ) -> Result<AgentOutcome, AgentError> {
        let ctx = SessionContext::new(
            options
                .session_id
                .as_deref()
                .unwrap_or(&SessionContext::default().into_session_id()),
        );
        let log = AgentLogger::from_context(&ctx);
        log.info("Agent run started");
        if let Some(ref limiter) = self.rate_limiter {
            let sid = options.session_id.as_deref().unwrap_or("default");
            limiter
                .check_rate_limit(sid)
                .await
                .map_err(|_| AgentError::RateLimited)?;
        }
        let mut session_id = options.session_id.clone();
        let mut steps = Vec::new();
        let mut logs = Vec::new();

        if let Some(ref sender) = options.event_sender {
            let _ = sender.send(DomainEvent::AgentRunStarted {
                session_id: session_id.clone(),
                prompt: prompt.clone(),
            });
        }

        let context = self.runtime.build_context(Some(&prompt), &ctx).await;
        let instructions = self
            .runtime
            .compose_system_instructions(&context, self.client.prompts());
        let system_prompt = match options.system_prompt.take() {
            Some(existing) if !existing.trim().is_empty() => {
                format!("{existing}\n\n{instructions}")
            }
            _ => instructions,
        };

        log.info(format!(
            "System prompt | chars={} preview={}",
            system_prompt.len(),
            McpClient::<P>::summarise(&system_prompt)
        ));

        let prompt_preview = McpClient::<P>::summarise(&prompt);
        let mut next_prompt = self.runtime.initial_user_prompt(prompt, &context);
        logs.push(format!("Initial agent request: {prompt_preview}"));

        let effective_provider = self.client.default_provider().to_string();
        let effective_model = self.client.default_model().to_string();
        logs.push(format!(
            "Active provider: '{effective_provider}' | Model: '{effective_model}'"
        ));

        let mut remaining_steps = options.max_steps;
        let mut system_prompt_to_send = Some(system_prompt);
        #[cfg(not(target_arch = "wasm32"))]
        let mut system = System::new();
        #[cfg(not(target_arch = "wasm32"))]
        let mut last_resource_check: std::time::Instant = std::time::Instant::now();
        #[cfg(not(target_arch = "wasm32"))]
        const RESOURCE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
        let mut first_call = true;
        let initial_attachments = std::mem::take(&mut options.attachments);

        loop {
            #[cfg(not(target_arch = "wasm32"))]
            if last_resource_check.elapsed() >= RESOURCE_CHECK_INTERVAL {
                system.refresh_cpu_all();
                system.refresh_memory();
                let rss_mb = system.used_memory() / 1024 / 1024;
                let cpu = system.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
                    / system.cpus().len().max(1) as f32;
                log.debug(format!(
                    "Agent resource utilization | rss_mb={} cpu_usage={}",
                    rss_mb, cpu
                ));
                last_resource_check = std::time::Instant::now();
            }

            log.debug(format!(
                "Submitting agent turn to model provider | session={:?} remaining_steps={}",
                session_id.as_deref(),
                remaining_steps
            ));
            let prompt_preview = McpClient::<P>::summarise(&next_prompt);
            log.info(format!(
                "-> Agent REQ | chars={} | {}",
                next_prompt.len(),
                prompt_preview
            ));
            let request = ChatRequest {
                prompt: next_prompt.clone(),
                attachments: if first_call {
                    initial_attachments.clone()
                } else {
                    Vec::new()
                },
                system_prompt: if first_call {
                    system_prompt_to_send.take()
                } else {
                    None
                },
                session_id: session_id.clone(),
                raw_mode: false,
                bypass_template: true,
                force_json: true,
            };

            if let Some(ref limiter) = self.rate_limiter {
                let sid = session_id.as_deref().unwrap_or("default");
                limiter
                    .check_rate_limit(sid)
                    .await
                    .map_err(|_| AgentError::RateLimited)?;
            }
            let result = self.client.chat(request).await?;
            logs.extend(result.logs.clone());
            session_id = Some(result.session_id.clone());
            first_call = false;

            if let Some(ref sender) = options.event_sender {
                let _ = sender.send(DomainEvent::SessionUpdated {
                    session_id: result.session_id.clone(),
                    message_count: steps.len(),
                });
            }

            let response_preview = McpClient::<P>::summarise(&result.content);
            log.info(format!(
                "<- Agent RES | chars={} | {}",
                result.content.len(),
                response_preview
            ));

            let directive = self
                .runtime
                .parse_with_retry(&result.content, &self.client, &mut logs, &session_id)
                .await?;

            match directive {
                AgentDirective::Final { response } => {
                    log.info(format!(
                        "Agent returned final response | session_id={}",
                        result.session_id.as_str()
                    ));
                    if let Some(ref sender) = options.event_sender {
                        let _ = sender.send(DomainEvent::AgentRunCompleted {
                            session_id: result.session_id.clone(),
                            response: serde_json::to_string(&response).unwrap_or_default(),
                            total_steps: steps.len(),
                        });
                    }
                    return Ok(AgentOutcome {
                        logs,
                        session_id: result.session_id,
                        response,
                        steps,
                    });
                }
                AgentDirective::CallTool { tool, input } => {
                    if remaining_steps == 0 {
                        log.warn("Agent exceeded max tool interactions");
                        return Err(AgentError::InvalidResponse(
                            self.client.prompts().agent_max_steps_error().into(),
                        ));
                    }
                    remaining_steps -= 1;
                    log.info(format!("Agent requested tool execution | tool={}", tool));
                    let execution = self.runtime.execute(&tool, input, &ctx).await?;
                    logs.push(format!(
                        "Tool '{}' executed (success: {})",
                        execution.tool, execution.success
                    ));
                    if let Some(message) = execution.message.as_deref() {
                        logs.push(format!(
                            "Tool message: {}",
                            McpClient::<P>::summarise(message)
                        ));
                    }

                    steps.push(AgentStep {
                        tool: execution.tool.clone(),
                        input: execution.input.clone(),
                        success: execution.success,
                        output: execution.output.clone(),
                        message: execution.message.clone(),
                    });

                    if let Some(ref sender) = options.event_sender {
                        let _ = sender.send(DomainEvent::ToolInvoked {
                            tool: execution.tool.clone(),
                            input: execution.input.clone(),
                            success: execution.success,
                        });
                        let _ = sender.send(DomainEvent::AgentStepCompleted {
                            step: AgentStep {
                                tool: execution.tool.clone(),
                                input: execution.input.clone(),
                                success: execution.success,
                                output: execution.output.clone(),
                                message: execution.message.clone(),
                            },
                            remaining_steps,
                        });
                    }

                    let tool_result_instruction = self.client.prompts().tool_result_instruction();
                    next_prompt = ToolResultParser::format_single(
                        execution.tool,
                        execution.input,
                        execution.success,
                        execution.output,
                        execution.message,
                        tool_result_instruction,
                    );
                }
                AgentDirective::CallTools(tools) => {
                    if remaining_steps == 0 {
                        log.warn("Agent exceeded max tool interactions");
                        return Err(AgentError::InvalidResponse(
                            self.client.prompts().agent_max_steps_error().into(),
                        ));
                    }
                    remaining_steps -= 1;
                    log.info(format!(
                        "Agent requested parallel tool execution | count={}",
                        tools.len()
                    ));

                    let executions = self.runtime.clone().execute_parallel(tools, &ctx).await?;
                    let mut aggregated_results = Vec::new();

                    for exec_result in executions {
                        match exec_result {
                            Ok(execution) => {
                                logs.push(format!(
                                    "Tool '{}' executed (success: {})",
                                    execution.tool, execution.success
                                ));
                                if let Some(message) = execution.message.as_deref() {
                                    logs.push(format!(
                                        "Tool message: {}",
                                        McpClient::<P>::summarise(message)
                                    ));
                                }

                                steps.push(AgentStep {
                                    tool: execution.tool.clone(),
                                    input: execution.input.clone(),
                                    success: execution.success,
                                    output: execution.output.clone(),
                                    message: execution.message.clone(),
                                });

                                if let Some(ref sender) = options.event_sender {
                                    let _ = sender.send(DomainEvent::ToolInvoked {
                                        tool: execution.tool.clone(),
                                        input: execution.input.clone(),
                                        success: execution.success,
                                    });
                                    let _ = sender.send(DomainEvent::AgentStepCompleted {
                                        step: AgentStep {
                                            tool: execution.tool.clone(),
                                            input: execution.input.clone(),
                                            success: execution.success,
                                            output: execution.output.clone(),
                                            message: execution.message.clone(),
                                        },
                                        remaining_steps,
                                    });
                                }

                                aggregated_results.push(ToolResultParser::single_result_value(
                                    execution.tool,
                                    execution.input,
                                    execution.success,
                                    execution.output,
                                    execution.message,
                                ));
                            }
                            Err(e) => {
                                log.warn(format!("One of the parallel tools failed: {}", e));
                                logs.push(format!("Parallel tool failure: {}", e));
                            }
                        }
                    }

                    let tool_result_instruction = self.client.prompts().tool_result_instruction();
                    next_prompt = ToolResultParser::format_parallel(
                        aggregated_results,
                        tool_result_instruction,
                    );
                }
            }
        }
    }

    /// Run the agent and replace response references with their tool result data.
    ///
    /// Returns the original execution outcome together with the processed response.
    /// Plain string responses are wrapped in a JSON object with a `content` field
    /// so callers receive the same object shape as structured model responses.
    pub async fn run_ui_layout(
        &self,
        prompt: String,
        options: AgentOptions,
    ) -> Result<(AgentOutcome, serde_json::Value), AgentError> {
        let outcome = self.run(prompt, options).await?;

        let processed_response =
            ResponseEmbedder::embed_tool_results_sync(outcome.response.clone(), &outcome.steps);

        let final_response = match processed_response {
            Value::String(s) => {
                json!({"content": s})
            }
            _ => processed_response,
        };

        Ok((outcome, final_response))
    }
}
