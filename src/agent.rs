use std::{collections::HashMap, sync::{Arc, RwLock}, u64};

use call_agent::chat::{client::{OpenAIClient, OpenAIClientState}, prompt::{Message, MessageContext, MessageImage}};
use log::debug;
use observer::prefix::{ASK_DEVELOPER_PROMPT, ASSISTANT_NAME, DEEP_SEARCH_DEVELOPER_PROMPT, DEEP_SEARCH_GENERATE_PROMPT, MAX_USE_TOOL_COUNT};
use regex::Regex;
use serenity::all::{Context, CreateMessage, MessageFlags};
use tokio::sync::Mutex;

use crate::fetch_and_encode_images;

#[derive(Clone, Debug)]
pub struct InputMessage {
    pub content: String,
    pub name: String,
    pub message_id: String,
    pub reply_msg: Option<String>,
    pub user_id: String,
    pub attached_files: Vec<String>,
}
// 各チャンネルの会話履歴（state）を保持する構造体
pub struct ChannelState {
    // 並列処理のため、prompt_stream を Mutex で保護する
    pub prompt_stream: Mutex<OpenAIClientState>,
}

impl ChannelState {
    pub async fn new(client: &Arc<OpenAIClient>) -> Self {
        // 新しい PromptStream を生成する
        let mut prompt_stream = client.create_prompt();
        prompt_stream.set_entry_limit(64).await;
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        Self {
            prompt_stream: Mutex::new(prompt_stream),
        }
    }

    async fn prepare_user_prompt(message: &mut InputMessage, viw_image_detail: u8) -> Vec<Message> {
        let re = Regex::new(r"(\|\|.*?\|\|)").unwrap();
        message.content = re.replace_all(&message.content, "||<spoiler_msg>||").to_string();

        // !hidetail が含まれていれば強制的に high detail
        let mut detail_flag = viw_image_detail;
        if message.content.contains("!hiimgv") {
            println!("hiimgv found in message content");
            detail_flag = 255;
            // 末尾／文中のフラグ文字列を削除
            message.content = message.content.replace("!hiimgv", "");
        }

        let meta = format!(
            "[META]msg_id:{},user_name:{},replay_msg:{};\n{}",
            message.message_id,
            message.name,
            message.reply_msg.clone().unwrap_or_else(|| "none".into()),
            message.content.clone(),
        );

        let mut content_vec = Vec::new();
        content_vec.push(MessageContext::Text(meta));

        // detail_flag に応じて画像を追加
        if detail_flag != 0 {
            // 画像を取得して data URL にした Vec<String>
            let img_urls = fetch_and_encode_images(&message.attached_files).await;

            for url in img_urls {
                let detail_str = match detail_flag {
                    1 => Some("low".to_string()),
                    255 => Some("high".to_string()),
                    _ => None,
                };
                content_vec.push(MessageContext::Image(MessageImage {
                    url,
                    detail: detail_str,
                }));
            }
        }

        vec![Message::User {
            content: content_vec,
            name: Some(message.user_id.clone()),
        }]
    }
    pub async fn ask(&self, mut message: InputMessage) -> String {
        let user_prompt = ChannelState::prepare_user_prompt(&mut message, 0).await;
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

    pub async fn reasoning(
        &self, 
        ctx: &Context,
        msg: &serenity::all::Message,
        mut message: InputMessage,
    ) -> String {
        // プロンプトストリームの取得
        let user_prompt = ChannelState::prepare_user_prompt(&mut message, 1).await;
        let mut r_prompt_stream = self.prompt_stream.lock().await;
        r_prompt_stream.add(user_prompt).await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream); // 先にロックを解除t_stream.clone();
        prompt_stream.set_entry_limit(u64::MAX).await;
        let last_pos = prompt_stream.prompt.len();

        // システムプロンプトの追加
        debug!("prompt_stream - {:#?}", prompt_stream.prompt);
        let system_prompt = vec![Message::Developer {
            content: ASK_DEVELOPER_PROMPT.to_string(),
            name: Some(ASSISTANT_NAME.to_string()),
        }];
        prompt_stream.add(system_prompt).await;


        let mut used_tools = Vec::new();

        for _ in 0..*MAX_USE_TOOL_COUNT {
            // 応答を生成
            let res = match prompt_stream.generate_can_use_tool(None).await {
                Ok(res) => {
                    res
                },
                Err(e) => {
                    return format!("Err: response is none from ai - {:?}", e);
                }
            };
            if res.has_tool_calls {
                // 推論の経過を表示
                let status_ms = format!(
                    "-#  {}\n-# using {}...",
                    res.content.unwrap_or_default(),
                    res.tool_calls.as_ref().unwrap().iter()
                        .map(|tool_call| tool_call.function.name.clone())
                        .collect::<Vec<String>>()
                        .join(", ")
                );
                let status_res = CreateMessage::new()
                    .content(status_ms)
                    .flags(MessageFlags::SUPPRESS_EMBEDS);

                if let Err(e) = msg.channel_id.send_message(&ctx.http, status_res).await {
                    debug!("Error sending message: {:?}", e);
                }
                
                // 使用されたツールの情報を収集
                res.tool_calls.unwrap().iter().for_each(|tool_call| {
                    used_tools.push(tool_call.function.name.clone());
                });
                continue;
            } else if res.has_content {
                // ツールコールがない場合の処理
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

    pub async fn add_message(&self, mut message: InputMessage) {
        let user_prompt = ChannelState::prepare_user_prompt(&mut message, 1).await;
        let mut prompt_stream = self.prompt_stream.lock().await;



        prompt_stream.add(user_prompt).await;
    }

    pub async fn clear_prompt(&self) {
        let mut prompt_stream = self.prompt_stream.lock().await;
        prompt_stream.clear().await;
    }
}