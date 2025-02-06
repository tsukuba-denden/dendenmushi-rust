use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;




































impl OpenAIClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        }
    }

    pub async fn call_function(
        &self,
        model: &str,
        prompt: &str,
        function_definitions: Vec<FunctionDef>,
    ) -> Result<Option<FunctionCall>, Box<dyn Error>> {
        let url = "https://api.openai.com/v1/chat/completions";

        let request = APIRequest {
            model: model.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            functions: function_definitions,
            function_call: "auto".to_string(), // 自動的に適切な関数を選ぶ
        };

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let response_body: APIResponse = res.json().await?;

        if let Some(function_call) = &response_body.choices[0].message.function_call {
            Ok(Some(function_call.clone()))
        } else {
            Ok(None)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "YOUR_OPENAI_API_KEY"; // ここにAPIキーを入れる
    let openai = OpenAIClient::new(api_key);

    // 関数定義: `get_weather` をモデルに伝える
    let function_definitions = vec![FunctionDefinition {
        name: "get_weather".to_string(),
        description: "Get the weather information for a given city.",
        parameters: json!({
            "type": "object",
            "properties": {
                "city": { "type": "string", "description": "The name of the city" },
                "unit": { "type": "string", "enum": ["C", "F"], "description": "Temperature unit (Celsius or Fahrenheit)" }
            },
            "required": ["city"]
        }),
    }];

    // Function Callingを試す
    if let Some(function_call) = openai
        .call_function("gpt-4-turbo", "東京の天気を教えて", function_definitions)
        .await?
    {
        println!("関数名: {}", function_call.name);
        println!("引数: {}", function_call.arguments);
    } else {
        println!("関数呼び出しなし");
    }

    Ok(())
}
