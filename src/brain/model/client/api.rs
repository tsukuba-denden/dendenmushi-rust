use serde::{Deserialize, Serialize};

use super::{prompt::{Choice, Message}, function::FunctionDef};

/// API headers構造体  
pub struct APIResponseHeaders {
    /// Retry-After  
    pub retry_after: Option<u64>,
    /// X-RateLimit-Reset  
    pub reset: Option<u64>,
    /// X-RateLimit-Remaining  
    pub rate_limit: Option<u64>,
    /// X-RateLimit-Limit  
    pub limit: Option<u64>,

    pub extra_other: Vec<(String, String)>,
}

/// APIリクエスト構造体  
#[derive(Debug, Serialize, Deserialize)]
pub struct APIRequest {
    /// モデル名の指定  
    /// ex. "GPT-4o"  
    pub model: String,
    /// プロンプトのメッセージ  
    pub messages: Vec<Message>,
    /// プロンプトで使用する関数の定義  
    pub functions: Vec<FunctionDef>,
    /// 関数の呼び出しの指定  
    /// ex. OpenAI API  
    /// - "auto"  
    /// - "none"  
    /// - { "name": "get_weather" }  
    pub function_call: serde_json::Value,
    /// 温度
    /// 0.0 ~ 1.0
    pub temperature: f64,
    /// 最大トークン数
    pub max_tokens: u64,
    pub top_p: f64,
}

/// レスポンス構造体
#[derive(Debug, Deserialize)]
pub struct APIResponse {
    pub choices: Option<Vec<Choice>>,
    pub model: Option<String>,
    pub object: Option<String>,
    pub error: Option<APIError>,
    pub usage: Option<APIUsage>,
}

#[derive(Debug, Deserialize)]
pub struct APIError {
    pub message: String,
    #[serde(rename = "type")]
    pub err_type: String,
    pub code: i32,
}

#[derive(Debug, Deserialize)]
pub struct APIUsage {
    /// プロンプトで使用されたトークン数  
    pub prompt_tokens: Option<u64>,
    /// 応答で使用されたトークン数  
    pub completion_tokens: Option<u64>,
    /// 合計トークン数  
    /// プロンプトと応答のトークン数の合計  
    pub total_tokens: Option<u64>,
}