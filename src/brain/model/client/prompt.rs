use reqwest::Response;
use serde::{Deserialize, Serialize};

use super::function::FunctionCall;

/// Promptのメッセージ構造体  
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: Vec<MessageContext>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MessageContext {
    Text(String),
    Image { image_url: MessageImage },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageImage {
    /// 画像のURL  
    /// url or  
    /// base64  
    /// ex. "data:image/jpeg;base64,'{IMAGE_DATA}'"  
    /// ex. "https://example.com/image.jpg"  
    pub url: String,
    /// 画像の解像度  
    /// ex open ai api  
    /// - "low"  
    /// - "medium"  
    /// - "auto" (default)  
    pub detail: Option<String>,
}





#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: ResponseMessage,
    pub finish_reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub function_call: Option<FunctionCall>,
}