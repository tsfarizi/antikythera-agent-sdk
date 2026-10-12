//! Canonical domain definitions for message roles, parts, and messages.
//!
//! These types represent the conversation primitives (`MessageRole`, `MessagePart`,
//! `Message`) used across the framework. They live in the domain layer so that
//! core never depends on an external session crate for its foundational types.

use serde::{Deserialize, Serialize};

/// Role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User input
    User,
    /// Assistant response
    Assistant,
    /// System message
    System,
    /// Tool call result
    ToolResult,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::ToolResult => "tool_result",
        }
    }
}

/// A part of a message content - can be text, image, or file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagePart {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        data: String,
    },
    File {
        name: String,
        mime_type: String,
        data: String,
    },
}

impl serde::Serialize for MessagePart {
    /// Postcard and other binary formats.
    ///
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        if serializer.is_human_readable() {
            match self {
                MessagePart::Text { text } => {
                    let mut state = serializer.serialize_struct("MessagePart", 2)?;
                    state.serialize_field("type", "text")?;
                    state.serialize_field("text", text)?;
                    state.end()
                }
                MessagePart::Image { mime_type, data } => {
                    let mut state = serializer.serialize_struct("MessagePart", 3)?;
                    state.serialize_field("type", "image")?;
                    state.serialize_field("mime_type", mime_type)?;
                    state.serialize_field("data", data)?;
                    state.end()
                }
                MessagePart::File {
                    name,
                    mime_type,
                    data,
                } => {
                    let mut state = serializer.serialize_struct("MessagePart", 4)?;
                    state.serialize_field("type", "file")?;
                    state.serialize_field("name", name)?;
                    state.serialize_field("mime_type", mime_type)?;
                    state.serialize_field("data", data)?;
                    state.end()
                }
            }
        } else {
            #[derive(Serialize)]
            enum BinaryPart<'a> {
                Text {
                    text: &'a String,
                },
                Image {
                    mime_type: &'a String,
                    data: &'a String,
                },
                File {
                    name: &'a String,
                    mime_type: &'a String,
                    data: &'a String,
                },
            }

            let bin_part = match self {
                MessagePart::Text { text } => BinaryPart::Text { text },
                MessagePart::Image { mime_type, data } => BinaryPart::Image { mime_type, data },
                MessagePart::File {
                    name,
                    mime_type,
                    data,
                } => BinaryPart::File {
                    name,
                    mime_type,
                    data,
                },
            };
            bin_part.serialize(serializer)
        }
    }
}

impl<'de> serde::Deserialize<'de> for MessagePart {
    /// Postcard and other binary formats.
    ///
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            #[derive(Deserialize)]
            struct RawPart {
                #[serde(rename = "type")]
                part_type: String,
                text: Option<String>,
                name: Option<String>,
                mime_type: Option<String>,
                data: Option<String>,
            }

            let raw = RawPart::deserialize(deserializer)?;
            match raw.part_type.as_str() {
                "text" => Ok(MessagePart::Text {
                    text: raw.text.unwrap_or_default(),
                }),
                "image" => Ok(MessagePart::Image {
                    mime_type: raw.mime_type.unwrap_or_default(),
                    data: raw.data.unwrap_or_default(),
                }),
                "file" => Ok(MessagePart::File {
                    name: raw.name.unwrap_or_default(),
                    mime_type: raw.mime_type.unwrap_or_default(),
                    data: raw.data.unwrap_or_default(),
                }),
                _ => Err(serde::de::Error::custom("unknown message part type")),
            }
        } else {
            #[derive(Deserialize)]
            enum BinaryPart {
                Text {
                    text: String,
                },
                Image {
                    mime_type: String,
                    data: String,
                },
                File {
                    name: String,
                    mime_type: String,
                    data: String,
                },
            }

            let bin_part = BinaryPart::deserialize(deserializer)?;
            match bin_part {
                BinaryPart::Text { text } => Ok(MessagePart::Text { text }),
                BinaryPart::Image { mime_type, data } => Ok(MessagePart::Image { mime_type, data }),
                BinaryPart::File {
                    name,
                    mime_type,
                    data,
                } => Ok(MessagePart::File {
                    name,
                    mime_type,
                    data,
                }),
            }
        }
    }
}

impl MessagePart {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text {
            text: content.into(),
        }
    }

    pub fn image(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::Image {
            mime_type: mime_type.into(),
            data: data.into(),
        }
    }

    pub fn file(
        name: impl Into<String>,
        mime_type: impl Into<String>,
        data: impl Into<String>,
    ) -> Self {
        Self::File {
            name: name.into(),
            mime_type: mime_type.into(),
            data: data.into(),
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Single message in chat history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Structured content parts, preserving non-text attachments when present.
    #[serde(default)]
    pub parts: Vec<MessagePart>,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Tool call info (if role is ToolResult)
    pub tool_name: Option<String>,
    /// Tool arguments (JSON encoded, if applicable)
    pub tool_args: Option<String>,
    /// Step number in agent flow
    pub step: Option<u32>,
    /// Additional metadata (JSON encoded)
    pub metadata: Option<String>,
}

impl Message {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            role: MessageRole::User,
            content: content.clone(),
            parts: vec![MessagePart::text(content)],
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_name: None,
            tool_args: None,
            step: None,
            metadata: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            role: MessageRole::Assistant,
            content: content.clone(),
            parts: vec![MessagePart::text(content)],
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_name: None,
            tool_args: None,
            step: None,
            metadata: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            role: MessageRole::System,
            content: content.clone(),
            parts: vec![MessagePart::text(content)],
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_name: None,
            tool_args: None,
            step: None,
            metadata: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(
        tool_name: impl Into<String>,
        content: impl Into<String>,
        args: Option<serde_json::Value>,
        step: u32,
    ) -> Self {
        let content = content.into();
        Self {
            role: MessageRole::ToolResult,
            content: content.clone(),
            parts: vec![MessagePart::text(content)],
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_name: Some(tool_name.into()),
            tool_args: args.map(|a| serde_json::to_string(&a).unwrap_or_default()),
            step: Some(step),
            metadata: None,
        }
    }

    /// Create a message with structured parts.
    pub fn with_parts(role: MessageRole, parts: Vec<MessagePart>) -> Self {
        let content = parts
            .iter()
            .filter_map(MessagePart::as_text)
            .collect::<Vec<_>>()
            .join("");

        Self {
            role,
            content,
            parts,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_name: None,
            tool_args: None,
            step: None,
            metadata: None,
        }
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| {
            let msg = format!("Serialize error: {}", e);
            msg
        })
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| {
            let msg = format!("Deserialize error: {}", e);
            msg
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/message_types.tests.rs"]
mod tests;
