use call_agent::chat::function::Tool;
use serde_json::Value;

/// **テキストの長さを計算するツール**
pub struct TextLengthTool;

impl TextLengthTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for TextLengthTool {
    fn def_name(&self) -> &str {
        "text_length_tool"
    }

    fn def_description(&self) -> &str {
        "Returns the length of the input text."
    }

    fn def_parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Input text to calculate its length"
                }
            },
            "required": ["text"]
        })
    }

    fn run(&self, args: Value) -> Result<String, String> {
        // JSONから"text"キーを取得
        let text = args["text"].as_str()
            .ok_or_else(|| "Missing 'text' parameter".to_string())?;
        
        // 長さを計算
        let length = text.len();

        // JSONで結果を返す
        Ok(serde_json::json!({ "length": length }).to_string())
    }
}