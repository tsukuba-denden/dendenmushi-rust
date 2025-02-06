use std::{collections::HashMap, f32::consts::E};

use reqwest::{Client, Response};

use super::{api::{APIRequest, APIResponse, APIResponseHeaders}, err::ClientError, function::{FunctionDef, Tool}, prompt::{Choice, Message}};

/// ルート構造体  
pub struct OpenAIClient {
    pub client: Client,
    pub end_point: String,
    pub api_key: Option<String>,
    pub tools: HashMap<String, (Box<dyn Tool>, bool/* enable */)>,
}

struct OpenAIClientState<'a> {
    pub prompt: Vec<Message>,
    pub client: &'a OpenAIClient,
}

impl OpenAIClient {
    /// OpenAIClientを生成  
    /// end_point: OpenAI APIのエンドポイント  
    /// api_key: OpenAI APIのAPIキー  
    pub fn new(end_point: &str,api_key: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            end_point: end_point.trim_end_matches('/').to_string(),
            api_key: api_key.map(|s| s.to_string()),
            tools: HashMap::new(),
        }
    }

    /// ツールを登録  
    /// tool: ツール
    /// T: Toolを実装した型
    /// ツール名が重複している場合は上書きされる
    pub fn def_tool<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.def_name().to_string(), (Box::new(tool), true));
    }

    /// ツールの一覧を取得  
    /// return: (ツール名, ツールの説明, 有効/無効)のリスト
    pub fn list_tools(&self) -> Vec<(String, String, bool)> {
        let mut tools = Vec::new();
        for (tool_name, (tool, enable)) in self.tools.iter() {
            tools.push((tool_name.to_string(), tool.def_description().to_string(), *enable));
        }
        tools
    }

    /// ツールの有効/無効を切り替え  
    /// tool_name: ツール名  
    /// t_enable: 有効/無効  
    pub fn switch_tool(&mut self, tool_name: &str, t_enable: bool) {
        if let Some((_, enable)) = self.tools.get_mut(tool_name) {
            *enable = t_enable;
        }
    }

    /// ツールの定義をエクスポート  
    pub fn export_tool_def(&self) -> Vec<FunctionDef> {
        let mut defs = Vec::new();
        for (tool_name, (tool, enable)) in self.tools.iter() {
            if *enable {
                defs.push(FunctionDef {
                    name: tool_name.to_string(),
                    description: tool.def_description().to_string(),
                    parameters: tool.def_parameters(),
                });
            }
        }
        defs
    }

    pub fn send_with_tool(&self, prompt: &Vec<Message>, tool_name: &str) -> Result<Choice, ClientError> {
        let tool = self.tools.get(tool_name).ok_or(ClientError::ToolNotFound)?;


        Err(ClientError::UnknownError)
    }

    /// APIを呼び出す  
    /// model: モデル名  
    /// - "GPT-4o"
    /// prompt: プロンプト  
    /// function_call: 関数呼び出し
    /// - "auto"  
    /// - "none"  
    /// - { "name": "get_weather" }  
    pub async fn call_api(&self, model: &str, prompt: &Vec<Message>, function_call: Option<&serde_json::Value>, temp: Option<f64>, max_token: Option<u64>, top_p: Option<f64>) -> Result<(APIResponse, APIResponseHeaders), ClientError> {
        let url = format!("{}/v1/chat/completions", self.end_point);
        if !url.starts_with("https://") {
            return Err(ClientError::InvalidEndpoint);
        }

        let request = APIRequest {
            model: model.to_string(),
            messages: prompt.clone(),
            functions: self.export_tool_def(),
            function_call: function_call.unwrap_or(&serde_json::Value::String("none".to_string())).clone(),
            temperature: temp.unwrap_or(0.5),
            max_tokens: max_token.unwrap_or(4000),
            top_p: top_p.unwrap_or(1.0),
        };

        let res = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await.map_err(|_| ClientError::NetworkError)?;

        let headers = APIResponseHeaders {
            retry_after: res.headers().get("Retry-After").and_then(|v| v.to_str().ok().and_then(|v| v.parse().ok())),
            reset: res.headers().get("X-RateLimit-Reset").and_then(|v| v.to_str().ok().and_then(|v| v.parse().ok())),
            rate_limit: res.headers().get("X-RateLimit-Remaining").and_then(|v| v.to_str().ok().and_then(|v| v.parse().ok())),
            limit: res.headers().get("X-RateLimit-Limit").and_then(|v| v.to_str().ok().and_then(|v| v.parse().ok())),
            extra_other: res.headers().iter().map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string())).collect(),
        };

        let response_body: APIResponse = res.json().await.map_err(|_| ClientError::InvalidResponse)?;

        Ok((response_body, headers))
    }

    /// プロンプトを生成  
    pub fn create_prompt(&self) -> OpenAIClientState {
        OpenAIClientState {
            prompt: Vec::new(),
            client: self,
        }
    }
}

impl<'a> OpenAIClientState<'a> {
    /// メッセージを追加  
    /// messages: メッセージのリスト  
    /// return: self  
    pub fn add(&mut self, messages: Vec<Message>) -> &mut Self {
        self.prompt.extend(messages);
        self
    }


}