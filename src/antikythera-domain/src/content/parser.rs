//! Parser for MCP tool output.

use super::types::{ContentItem, FileContent};
use serde_json::Value;

/// Parsed output from a tool step.
#[derive(Debug, Clone, Default)]
pub struct ParsedOutput {
    /// Text messages from the output
    pub texts: Vec<String>,
    /// File contents extracted from resources
    pub files: Vec<FileContent>,
    /// Raw output value (preserved for backward compatibility)
    pub raw: Value,
}

impl ParsedOutput {
    /// Check if there are any files in the output.
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    /// Get the first text message.
    pub fn first_text(&self) -> Option<&String> {
        self.texts.first()
    }

    /// Get combined text messages.
    pub fn combined_text(&self) -> String {
        self.texts.join("\n")
    }

    /// Get PDF files only.
    pub fn pdf_files(&self) -> Vec<&FileContent> {
        self.files.iter().filter(|f| f.is_pdf()).collect()
    }

    /// Get image files only.
    pub fn image_files(&self) -> Vec<&FileContent> {
        self.files.iter().filter(|f| f.is_image()).collect()
    }
}

/// Parse raw tool output into structured format.
/// Try to parse as MCP tool result with content array.
///
pub fn parse_step_output(output: &Value, created_at: &str) -> ParsedOutput {
    let mut parsed = ParsedOutput {
        raw: output.clone(),
        ..Default::default()
    };

    if let Some(content) = output.get("content").and_then(|c| c.as_array()) {
        for item in content {
            if let Ok(content_item) = serde_json::from_value::<ContentItem>(item.clone()) {
                if content_item.is_text() {
                    if let Some(text) = &content_item.text {
                        parsed.texts.push(text.clone());
                    }
                } else if content_item.is_resource()
                    && let Some(file) = content_item.to_file_content(created_at)
                {
                    parsed.files.push(file);
                }
            }
        }
    }

    parsed
}
