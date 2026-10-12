//! Content types mirroring MCP server types.

use serde::{Deserialize, Serialize};

/// Metadata for file content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    /// Original filename with extension
    pub filename: String,
    /// MIME type (e.g., "application/pdf")
    pub mime_type: String,
    /// File size in bytes
    pub size_bytes: usize,
    /// Creation timestamp in ISO8601 format
    pub created_at: String,
}

/// File content with metadata and base64-encoded data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// File metadata
    pub metadata: FileMetadata,
    /// Base64-encoded file data
    pub data: String,
}

impl FileContent {
    /// Check if this is a PDF file.
    pub fn is_pdf(&self) -> bool {
        self.metadata.mime_type == "application/pdf"
    }

    /// Check if this is an image file.
    pub fn is_image(&self) -> bool {
        self.metadata.mime_type.starts_with("image/")
    }

    /// Get file extension from filename.
    pub fn extension(&self) -> Option<&str> {
        self.metadata.filename.rsplit('.').next()
    }
}

/// Content item from MCP tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    /// Content type ("text" or "resource")
    #[serde(rename = "type")]
    pub content_type: String,
    /// Text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64-encoded data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// MIME type
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// File metadata (extended field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FileMetadata>,
}

impl ContentItem {
    /// Check if this is a text content.
    pub fn is_text(&self) -> bool {
        self.content_type == "text"
    }

    /// Check if this is a resource/file content.
    pub fn is_resource(&self) -> bool {
        self.content_type == "resource"
    }

    /// Convert to FileContent if this is a resource.
    /// `created_at` must be provided by the caller (injected, not global clock).
    /// Use provided metadata or create from available info.
    ///
    pub fn to_file_content(&self, created_at: &str) -> Option<FileContent> {
        if !self.is_resource() {
            return None;
        }

        let data = self.data.as_ref()?;
        let mime_type = self
            .mime_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let metadata = self.metadata.clone().unwrap_or_else(|| {
            let filename = self
                .text
                .as_ref()
                .and_then(|t| t.strip_prefix("Generated file: "))
                .unwrap_or("unknown")
                .to_string();

            FileMetadata {
                filename,
                mime_type: mime_type.clone(),
                size_bytes: data.len(),
                created_at: created_at.to_string(),
            }
        });

        Some(FileContent {
            metadata,
            data: data.clone(),
        })
    }
}
