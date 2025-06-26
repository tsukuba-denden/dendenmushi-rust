
use call_agent::chat::{client::{OpenAIClient, ToolMode}, function::Tool, prompt::{Message, MessageContext}};
use log::info;
use serde_json::Value;
use tokio::runtime::Runtime;


/// **テキストの長さを計算するツール**
pub struct BrowsingWorker {
    pub model: OpenAIClient,
}

impl BrowsingWorker {
    pub fn new(model: OpenAIClient) -> Self {
        Self { model }
    }
}

impl Tool for BrowsingWorker {
    fn def_name(&self) -> &str {
        "browsing_worker"
    }

    fn def_description(&self) -> &str {
        "Get a summary of the web page. If you want to obtain the original page without summarization, please use your browser. You can also provide instructions in natural language along with the URL. It can generate a summary of the entire page very quickly. Please note that others cannot see your response."
    }

    fn def_parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "URL and some natural language query ex.Gather links to the materials located at https://*.*/*/... . please write with user used language eg."
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
        info!("BrowsingWorker::run called with args: {:?}", args);
        let query = args["query"].as_str()
            .ok_or_else(|| "Missing 'query' parameter".to_string())?
            .to_string();

        let mut model = self.model.clone().create_prompt();

        let result = std::thread::spawn(move || -> Result<String, String> {
            let rt = Runtime::new().expect("Failed to create runtime");
            let messages = Vec::from(vec![
                Message::System { 
                    name: Some("owner".to_string()), 
                    content: "You are an excellent AI assistant who searches for web pages regarding the request content and faithfully summarizes the entire content of that page. Absolutely use the internet to research and compile information.Also, be sure to indicate the source (URL).".to_string() 
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
                let return_value = model.generate(None).await.map_err(|_| "Failed to generate".to_string())?;
                let mut string = return_value.content.ok_or("Failed to result".to_string())?;
                let annotations = &return_value.api_result.response.choices
                    .unwrap()[0].message.annotations;
                let captions = annotations.as_ref()
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| 
                        v.as_object()
                        .unwrap()
                        .get("url_citation")
                        .unwrap()
                        .as_object()
                        .unwrap()
                        .get("url")
                        .unwrap()
                        .as_str()
                        .unwrap_or("")
                        )
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_string();
                string = format!("{}\n\nLinks: {}", string, captions);
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