use serde::{Deserialize, Serialize};

/// Prompt template variables
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptVariables {
    pub custom_instruction: Option<String>,
    pub language_guidance: Option<String>,
    pub tool_guidance: Option<String>,
    pub json_schema: Option<String>,
}

impl PromptVariables {
    /// Render template with variables
    pub fn render(&self, template: &str) -> String {
        let mut result = template.to_string();

        if let Some(val) = &self.custom_instruction {
            result = result.replace("{{custom_instruction}}", val);
        } else {
            result = result.replace("{{custom_instruction}}\n\n", "");
        }

        if let Some(val) = &self.language_guidance {
            result = result.replace("{{language_guidance}}", val);
        } else {
            result = result.replace("\n\n{{language_guidance}}", "");
        }

        if let Some(val) = &self.tool_guidance {
            result = result.replace("{{tool_guidance}}", val);
        } else {
            result = result.replace("\n\n{{tool_guidance}}", "");
        }

        if let Some(val) = &self.json_schema {
            result.push_str("\n\n");
            result.push_str(val);
        }

        result
    }
}
