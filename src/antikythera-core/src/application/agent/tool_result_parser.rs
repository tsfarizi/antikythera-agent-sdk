use serde_json::{Value, json};

pub struct ToolResultParser;

impl ToolResultParser {
    pub fn format_single(
        tool: String,
        input: Value,
        success: bool,
        output: Value,
        message: Option<String>,
        instruction: &str,
    ) -> String {
        json!({
            "tool_result": {
                "tool": tool,
                "input": input,
                "success": success,
                "output": output,
                "message": message,
            },
            "instruction": instruction,
        })
        .to_string()
    }

    pub fn format_parallel(results: Vec<Value>, instruction: &str) -> String {
        json!({
            "tool_results": results,
            "instruction": instruction,
        })
        .to_string()
    }

    pub fn single_result_value(
        tool: String,
        input: Value,
        success: bool,
        output: Value,
        message: Option<String>,
    ) -> Value {
        json!({
            "tool": tool,
            "input": input,
            "success": success,
            "output": output,
            "message": message,
        })
    }
}

#[cfg(test)]
#[path = "../../../../../tests/rust/unit/antikythera-core/application/agent/tool_result_parser.tests.rs"]
mod tests;
