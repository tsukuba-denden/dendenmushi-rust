use std::{collections::HashMap, sync::{Arc, RwLock}, u64};

use call_agent::chat::{client::{OpenAIClient, OpenAIClientState}, prompt::{Message, MessageContext}};
use log::debug;
use observer::prefix::{ASK_DEVELOPER_PROMPT, ASSISTANT_NAME, DEEP_SEARCH_DEVELOPER_PROMPT, DEEP_SEARCH_GENERATE_PROMPT, MAX_USE_TOOL_COUNT};
use regex::Regex;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct InputMessage {
    pub content: String,
    pub name: String,
    pub message_id: String,
    pub reply_to: Option<String>,
    pub user_id: String,
}
// 各チャンネルの会話履歴（state）を保持する構造体
pub struct ChannelState {
    // 並列処理のため、prompt_stream を Mutex で保護する
    prompt_stream: Mutex<OpenAIClientState>,
    enable: RwLock<bool>,
}

impl ChannelState {
    pub async fn new(client: &Arc<OpenAIClient>) -> Self {
        // 新しい PromptStream を生成する
        let mut prompt_stream = client.create_prompt();
        prompt_stream.set_entry_limit(128).await;
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        Self {
            prompt_stream: Mutex::new(prompt_stream),
            enable: RwLock::new(false),
        }
    }

    pub async fn enable(&self) {
        let mut enable = self.enable.write().unwrap();
        *enable = true;
    }

    pub async fn disable(&self) {
        let mut enable = self.enable.write().unwrap();
        *enable = false;
    }

    pub async fn ask(&self, mut message: InputMessage) -> String {
        if !*self.enable.read().unwrap() {
            return "Info: AI is disable. Type '/enable' to enable it".to_string();
        }

        let re = Regex::new(r"(\|\|.*?\|\|)").unwrap();
        message.content = re.replace_all(&message.content, "").to_string();
        let meta = format!("<meta>message_id:{},user_name,:{}, replay_to_id:{}</meta>", message.message_id, message.name, message.reply_to.unwrap_or("none".to_string()));
        let user_prompt = vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content),
            ],
            name: Some(message.user_id),
        }];
        let mut r_prompt_stream = self.prompt_stream.lock().await;
        r_prompt_stream.add(user_prompt).await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream); // 先にロックを解除t_stream.clone();
        prompt_stream.set_entry_limit(u64::MAX).await;
        let last_pos = prompt_stream.prompt.len();

        debug!("prompt_stream - {:#?}", prompt_stream.prompt);
        let system_prompt = vec![Message::Developer {
            content: ASK_DEVELOPER_PROMPT.to_string(),
            name: Some(ASSISTANT_NAME.to_string()),
        }];

        prompt_stream.add(system_prompt).await;

        let mut used_tools = Vec::new();

        for _ in 0..*MAX_USE_TOOL_COUNT {
            let res = match prompt_stream.generate_can_use_tool(None).await {
                Ok(res) => {
                    res
                },
                Err(e) => {
                    return format!("Err: response is none from ai - {:?}", e);
                }
            };
            if res.has_tool_calls {
                res.tool_calls.unwrap().iter().for_each(|tool_call| {
                    used_tools.push(tool_call.function.name.clone());
                });
                continue;
            } else if res.has_content {
                let tag = format!("\n-# model: {}", prompt_stream.client.model_config.unwrap().model);
                let mut tool_count = HashMap::new();
                for tool in used_tools {
                    *tool_count.entry(tool).or_insert(0) += 1;
                }
                let used_tools_info = if !tool_count.is_empty() {
                    let tools_info: Vec<String> = tool_count.iter().map(|(tool, count)| {
                        if *count > 1 {
                            format!("{} x{}", tool, count)
                        } else {
                            tool.clone()
                        }
                    }).collect();
                    format!("\n-# tools: {}", tools_info.join(", "))
                } else {
                    "".to_string()
                };
                let differential_stream = prompt_stream.prompt.split_off(last_pos + 1 /* 先頭のシステムプロンプト消す */);
                {
                    let mut r_prompt_stream = self.prompt_stream.lock().await;
                    r_prompt_stream.add(differential_stream.into()).await;
                }
                return res.content.unwrap().replace("\\n", "\n") + &tag + &used_tools_info;
            } else {
                return "Err: response is none from ai".to_string();
            }
        }
        let res = match prompt_stream.generate(None).await {
            Ok(res) => res,
            Err(_) => {
                return "Err: response is none from ai".to_string();
            }
        };
        if res.has_content {
            return res.content.unwrap().replace("\\n", "\n");
        } else {
            return "Err: response is none from ai".to_string();
        }
    }

    pub async fn deep_search(&self, message: InputMessage, try_count: usize) -> String {
        if !*self.enable.read().unwrap() {
            return "Info: AI is disable. Type '/enable' to enable it".to_string();
        }
        let r_prompt_stream = self.prompt_stream.lock().await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream);

        let meta = format!("<meta>message_id:{},user_name,:{}, replay_to_id:{}</meta>", message.message_id, message.name, message.reply_to.unwrap_or("none".to_string()));
        let user_prompt = vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content.clone()),
            ],
            name: Some(message.user_id),
        }];
        let system_prompt = vec![Message::Developer {
            content: DEEP_SEARCH_DEVELOPER_PROMPT
                .to_string(),
            name: Some(ASSISTANT_NAME.to_string()),
        }];

        prompt_stream.add(user_prompt).await;
        prompt_stream.add(system_prompt).await;

        for _ in 0..try_count {
            let _res = match  prompt_stream.generate_with_tool(None, "web_scraper").await {
                Ok(res) => res,
                Err(_) => {
                    return "Err: response is none from ai".to_string();
                }
            };
        }

        prompt_stream
            .add(vec![Message::Developer {
                content: format!("{}'{}'", *DEEP_SEARCH_GENERATE_PROMPT ,message.content),
                name: Some(ASSISTANT_NAME.to_string()),
            }])
            .await;
        let res = match prompt_stream.generate(None).await {
            Ok(res) => res,
            Err(_) => {
                return "Err: response is none from ai".to_string();
            }
        };
        if res.has_content {
            return res.content.unwrap().replace("\\n", "\n");
        } else {
            return "Err: response is none from ai".to_string();
        }
    }

    pub async fn add_message(&self, mut message: InputMessage) {
        if !*self.enable.read().unwrap() {
            return;
        }
        let re = Regex::new(r"(\|\|.*?\|\|)").unwrap();
        message.content = re.replace_all(&message.content, "").to_string();
        let mut prompt_stream = self.prompt_stream.lock().await;

        let meta = format!(
            "<meta>message_id:{},user_name:{}, replay_to_id:{}</meta>",
            message.message_id,
            message.name,
            message.reply_to.unwrap_or("none".to_string())
        );

        let prompt = 
            vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content),
            ],
            name: Some(message.user_id),
            }];

        prompt_stream.add(prompt).await;
    }

    pub async fn clear_prompt(&self) {
        let mut prompt_stream = self.prompt_stream.lock().await;
        prompt_stream.clear().await;
    }
}