use std::sync::Arc;

use call_agent::chat::{client::{OpenAIClient, ToolMode}, function::Tool, prompt::{Message, MessageContext}};
use serde_json::Value;
use tokio::runtime::Runtime;

use super::web_scraper::Browser;

/// **テキストの長さを計算するツール**
pub struct BrowsingWorker {
    pub model: OpenAIClient,
}

impl BrowsingWorker {
    pub fn new(mut model: OpenAIClient) -> Self {
        model.def_tool(Arc::new(Browser::new()));
        Self { model }
    }
}

impl Tool for BrowsingWorker {
    fn def_name(&self) -> &str {
        "browsing_worker"
    }

    fn def_description(&self) -> &str {
        "Get a summary of the web page. If you want to obtain the original page without summarization, please use your browser. You can also provide instructions in natural language along with the URL. It can generate a summary of the entire page very quickly."
    }

    fn def_parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "URL and some natural language query ex.Gather links to the materials located at https://*.*/*/..."
                },
                "$explain": {
                    "type": "string",
                    "description": "A brief explanation of what you are doing with this tool."
                },
            },
            "required": ["query"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        let query = args["query"].as_str()
            .ok_or_else(|| "Missing 'query' parameter".to_string())?
            .to_string();

        let mut model = self.model.clone().create_prompt();

        let result = std::thread::spawn(move || -> Result<String, String> {
            let rt = Runtime::new().expect("Failed to create runtime");
            let messages = Vec::from(vec![
                Message::System { 
                    name: Some("owner".to_string()), 
                    content: "You are an excellent AI assistant who searches for web pages regarding the request content and faithfully summarizes the entire content of that page. Please use the specified URL. Select everything except for script with CSS selectors. Set seek_pos = 0 and max_length = 200000. No `explanation` is needed.".to_string() 
                },
                Message::User {
                    name: Some("observer".to_string()),
                    content: vec![
                        MessageContext::Text(query.clone()),
                    ],
                }
            ]);

            // モデルに投げる
            let res: String = rt.block_on(async {
                model.add(messages).await;
                let mut reasoning_stream = model.reasoning(None, &ToolMode::Force("browser".to_string())).await.map_err(|_| "Failed call worker".to_string())?;
                reasoning_stream.proceed(&ToolMode::Disable).await.map_err(|_| "Failed to proceed".to_string())?;
                let string = reasoning_stream.content.ok_or("Failed to result".to_string())?;
                Ok(string)
            }).map_err(|e: String| e.to_string())?;

            Ok(res)
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())??;

        // JSONで結果を返す
        Ok(serde_json::json!({ "Summary": result }).to_string())
    }
}