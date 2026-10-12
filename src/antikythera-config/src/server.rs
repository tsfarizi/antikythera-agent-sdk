//! # MCP Server Configuration
//!
//! This module defines configuration for connecting to MCP (Model Context Protocol) servers.
//! MCP servers provide tools that the AI agent can use to perform actions.

pub use super::schema::{ServerConfig, TransportType};

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct RawServer {
    pub name: String,
    /// Command for STDIO transport (optional if url is provided)
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub workdir: Option<String>,
    /// URL for HTTP transport (optional if command is provided)
    pub url: Option<String>,
    /// HTTP headers for authentication
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub default_timezone: Option<String>,
    #[serde(default)]
    pub default_city: Option<String>,
}

impl From<RawServer> for ServerConfig {
    /// Select HTTP when a URL is configured, otherwise select stdio when a
    /// command is configured, or builtin when neither field is present.
    /// Native builds expand environment variables and home paths in commands,
    /// working directories, and arguments; WASM builds retain the input text.
    fn from(raw: RawServer) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let expand = |s: &str| -> String {
            shellexpand::full(s)
                .map(|cow| cow.into_owned())
                .unwrap_or_else(|_| s.to_string())
        };
        #[cfg(target_arch = "wasm32")]
        let expand = |s: &str| -> String { s.to_string() };

        let (transport, command, url) = if let Some(url_str) = raw.url {
            (TransportType::Http, None, Some(url_str))
        } else if let Some(cmd_str) = raw.command {
            let command_expanded = PathBuf::from(expand(&cmd_str));
            (TransportType::Stdio, Some(command_expanded), None)
        } else {
            (TransportType::Builtin, None, None)
        };

        let workdir = raw.workdir.map(|d| PathBuf::from(expand(&d)));
        let args = raw.args.into_iter().map(|arg| expand(&arg)).collect();

        Self {
            name: raw.name,
            transport,
            command,
            args,
            env: raw.env,
            workdir,
            url,
            headers: raw.headers,
            default_timezone: raw.default_timezone,
            default_city: raw.default_city,
        }
    }
}
