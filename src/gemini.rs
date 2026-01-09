use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{context::ObserverContext, lmclient::{LMContext, LMTool}};

#[derive(Clone)]
pub struct GeminiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model_name: String,
}

impl GeminiClient {
    pub fn new(base_url: String, api_key: String, model_name: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");

        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model_name,
        }
    }

    pub async fn generate(
        &self,
        ob_ctx: ObserverContext,
        lm_context: &LMContext,
        max_output_tokens: u32,
        tools: Option<std::sync::Arc<std::collections::HashMap<String, Box<dyn LMTool>>>>,
        mut state_send: impl FnMut(String),
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (system_instruction, base_contents) = convert_context(lm_context);

        let mut accumulated_text = String::new();
        let mut extra_contents: Vec<Content> = Vec::new();

        // function calling ループ（最大10手）
        for step in 0..10 {
            state_send(format!("Thinking... (gemini step {}/{})", step + 1, 10));

            let mut contents = Vec::with_capacity(base_contents.len() + extra_contents.len());
            contents.extend(base_contents.clone());
            contents.extend(extra_contents.clone());

            let req = GenerateContentRequest {
                system_instruction: system_instruction
                    .as_ref()
                    .map(|s| Content { role: None, parts: vec![Part::text(s.clone())] }),
                contents,
                generation_config: Some(GenerationConfig {
                    max_output_tokens: Some(max_output_tokens),
                }),
                tools: tools.as_ref().map(|t| vec![Tool {
                    function_declarations: t
                        .values()
                        .map(|tool| FunctionDeclaration {
                            name: tool.name(),
                            description: Some(tool.description()),
                            parameters: tool.json_schema(),
                        })
                        .collect(),
                }]),
            };

            let resp = self.post_generate(req).await?;
            let candidate = resp
                .candidates
                .into_iter()
                .next()
                .ok_or_else(|| std::io::Error::other("gemini: empty candidates"))?;

            let mut function_calls = Vec::new();
            let mut step_text = String::new();

            for part in candidate.content.parts {
                if let Some(text) = part.text {
                    step_text.push_str(&text);
                }
                if let Some(fc) = part.function_call {
                    function_calls.push(fc);
                }
            }

            if !step_text.is_empty() {
                accumulated_text.push_str(&step_text);
                // モデル発話として履歴に残す
                extra_contents.push(Content {
                    role: Some("model".to_string()),
                    parts: vec![Part::text(step_text)],
                });
            }

            if function_calls.is_empty() {
                break;
            }

            // まず「モデルが関数呼び出しを要求した」こと自体を履歴に残す
            for fc in &function_calls {
                extra_contents.push(Content {
                    role: Some("model".to_string()),
                    parts: vec![Part {
                        text: None,
                        function_call: Some(fc.clone()),
                        function_response: None,
                    }],
                });
            }

            // 関数実行 → functionResponse を user ロールで返す
            for fc in function_calls {
                let name = fc.name.clone();
                let args = fc
                    .args
                    .clone()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                let output = if let Some(tool_map) = tools.as_ref() {
                    if let Some(tool) = tool_map.get(&name) {
                        tool.execute(args, ob_ctx.clone()).await.unwrap_or_else(|e| format!("Error: {}", e))
                    } else {
                        format!("Error: tool not found: {}", name)
                    }
                } else {
                    format!("Error: tools disabled (requested: {})", name)
                };

                extra_contents.push(Content {
                    role: Some("user".to_string()),
                    parts: vec![Part {
                        text: None,
                        function_call: None,
                        function_response: Some(FunctionResponse {
                            name,
                            response: serde_json::json!({"output": output}),
                        }),
                    }],
                });
            }
        }

        if accumulated_text.trim().is_empty() {
            // 何もテキストが返ってこないケース
            Ok("(no output)".to_string())
        } else {
            Ok(accumulated_text)
        }
    }

    async fn post_generate(
        &self,
        req: GenerateContentRequest,
    ) -> Result<GenerateContentResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/models/{}:generateContent",
            self.base_url,
            self.model_name
        );

        // AI Studio API は `?key=` 方式
        let res = self
            .http
            .post(url)
            .query(&[("key", &self.api_key)])
            .json(&req)
            .send()
            .await?;

        if res.status() != StatusCode::OK {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            // キーを出さない
            return Err(Box::new(std::io::Error::other(format!(
                "gemini http error: {}: {}",
                status,
                body
            ))));
        }

        Ok(res.json::<GenerateContentResponse>().await?)
    }
}

fn convert_context(lm_context: &LMContext) -> (Option<String>, Vec<Content>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut contents: Vec<Content> = Vec::new();

    for item in lm_context.buf.iter() {
        let openai_dive::v1::resources::response::request::ResponseInputItem::Message(msg) = item else {
            continue;
        };

        match msg.role {
            openai_dive::v1::resources::response::response::Role::System => {
                // systemInstruction に寄せる
                if let Some(s) = content_to_text(&msg.content) {
                    if !s.trim().is_empty() {
                        system_parts.push(s);
                    }
                }
            }
            openai_dive::v1::resources::response::response::Role::User => {
                contents.push(Content {
                    role: Some("user".to_string()),
                    parts: vec![Part::text(content_to_text(&msg.content).unwrap_or_default())],
                });
            }
            openai_dive::v1::resources::response::response::Role::Assistant => {
                contents.push(Content {
                    role: Some("model".to_string()),
                    parts: vec![Part::text(content_to_text(&msg.content).unwrap_or_default())],
                });
            }
            _ => {}
        }
    }

    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n"))
    };

    (system_instruction, contents)
}

fn content_to_text(content: &openai_dive::v1::resources::response::request::ContentInput) -> Option<String> {
    use openai_dive::v1::resources::response::request::ContentItem;
    use openai_dive::v1::resources::response::request::ContentInput;

    match content {
        ContentInput::Text(t) => Some(t.clone()),
        ContentInput::List(items) => {
            let mut out = String::new();
            for item in items {
                match item {
                    ContentItem::Text { text } => {
                        out.push_str(text);
                    }
                    ContentItem::Image { image_url: Some(url), .. } => {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(&format!("[image] {}", url));
                    }
                    _ => {}
                }
            }
            Some(out)
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content>,
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,

    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    function_call: Option<FunctionCall>,

    #[serde(rename = "functionResponse", skip_serializing_if = "Option::is_none")]
    function_response: Option<FunctionResponse>,
}

impl Part {
    fn text(text: String) -> Self {
        Self {
            text: Some(text),
            function_call: None,
            function_response: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FunctionCall {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Tool {
    function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FunctionDeclaration {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Content,
}
