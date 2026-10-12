mod context;
mod execution;
mod instructions;
pub(super) mod json_retry;
mod parser;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::application::config::ToolConfig;

pub(super) use super::context::{ServerGuidance, ToolContext, ToolDescriptor};
pub(super) use super::directive::AgentDirective;
pub(super) use super::errors::{AgentError, ToolError};
pub(super) use crate::application::tooling::{ToolInvokeError, ToolServerInterface};
pub(super) use serde_json::{Value, json};
#[derive(Clone)]
pub struct ToolRuntime {
    configs: Vec<ToolConfig>,
    index: HashMap<String, ToolConfig>,
    bridge: Arc<dyn ToolServerInterface>,
    execution_semaphore: Arc<Semaphore>,
    pub(super) fallback_response_keys: Vec<String>,
}

impl ToolRuntime {
    /// Default limit to 10 concurrent tools.
    ///
    pub fn new(configs: Vec<ToolConfig>, bridge: Arc<dyn ToolServerInterface>) -> Self {
        let index = configs
            .iter()
            .cloned()
            .map(|cfg| (cfg.name.to_lowercase(), cfg))
            .collect();

        Self {
            configs,
            index,
            bridge,
            execution_semaphore: Arc::new(Semaphore::new(10)),
            fallback_response_keys: vec!["response".into(), "content".into(), "message".into()],
        }
    }

    /// Override the fallback response keys used when parsing unknown action values.
    pub fn with_fallback_keys(mut self, keys: Vec<String>) -> Self {
        if !keys.is_empty() {
            self.fallback_response_keys = keys;
        }
        self
    }
}
