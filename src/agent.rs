use std::{collections::HashMap, sync::Arc, u64};

use call_agent::chat::{
    client::{ModelConfig, OpenAIClient, OpenAIClientState, ToolMode},
    prompt::{Message, MessageContext, MessageImage},
};
use log::{debug, info};
use observer::prefix::{
    ASK_DEVELOPER_PROMPT, ASSISTANT_NAME, MAX_USE_TOOL_COUNT, MODEL_GENERATE_MAX_TOKENS,
};
use regex::Regex;
use serenity::all::{Context, CreateMessage, MessageFlags};
use tokio::sync::Mutex;

use crate::fetch_and_encode_images;

pub const PROMPT_ENTRY_LIMIT: u64 = 64; // プロンプトのエントリ数の上限

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

/// モデルの種類を指定するための列挙型
#[derive(Clone, Debug)]
pub enum AIModel {
    // MO3,
    MO4Mini,
    MO4MiniDeepResearch, // 追加のモデル例
    MO3,
    M4dot1Nano,
    M4dot1Mini,
    M4dot1,
    Gemini2dot5Flash,
}

impl AIModel {
    pub fn to_model_name(&self) -> String {
        match self {
            // AIModel::MO3 => "o3".to_string(),
            AIModel::MO4Mini => "o4-mini".to_string(),
            AIModel::MO4MiniDeepResearch => "o4-mini-deep-research".to_string(),
            AIModel::MO3 => "o3".to_string(),
            AIModel::M4dot1Nano => "gpt-4.1-nano".to_string(),
            AIModel::M4dot1Mini => "gpt-4.1-mini".to_string(),
            AIModel::M4dot1 => "gpt-4.1".to_string(),
            AIModel::Gemini2dot5Flash => "gemini-2.5-flash".to_string(),
        }
    }

    pub fn to_model_discription(&self) -> String {
        match self {
            // AIModel::MO3 => "Observer O3".to_string(),
            AIModel::MO4Mini => "o4-mini: late=4 4いつもの 数学とコーディングに強い".to_string(),
            AIModel::MO4MiniDeepResearch => {
                "o4-mini-deep-research: late=4 いつもの 深いリサーチが得意".to_string()
            }
            AIModel::MO3 => "o3: late=10 openAIの最強モデル".to_string(),
            AIModel::M4dot1Nano => "gpt-4.1-nano: late=1 超高速応答".to_string(),
            AIModel::M4dot1Mini => "gpt-4.1-mini: late=2 高速応答".to_string(),
            AIModel::M4dot1 => "gpt-4.1: late=10 一般".to_string(),
            AIModel::Gemini2dot5Flash => "gemini-2.5-flash: late=3 Googleの高速モデル".to_string(),
        }
    }

    pub fn to_sec_per_rate(&self) -> usize {
        match self {
            // AIModel::MO3 => 1,
            AIModel::MO4Mini => 4,
            AIModel::MO4MiniDeepResearch => 20,
            AIModel::MO3 => 10,
            AIModel::M4dot1Nano => 1,
            AIModel::M4dot1Mini => 2,
            AIModel::M4dot1 => 10,
            AIModel::Gemini2dot5Flash => 3,
        }
    }

    pub fn from_model_name(model_name: &str) -> Result<Self, String> {
        match model_name {
            // "o3" => Ok(AIModel::MO3),
            "o4-mini" => Ok(AIModel::MO4Mini),
            "o4-mini-deep-research" => Ok(AIModel::MO4MiniDeepResearch),
            "o3" => Ok(AIModel::MO3),
            "gpt-4.1-nano" => Ok(AIModel::M4dot1Nano),
            "gpt-4.1-mini" => Ok(AIModel::M4dot1Mini),
            "gpt-4.1" => Ok(AIModel::M4dot1),
            "gemini-2.5-flash" => Ok(AIModel::Gemini2dot5Flash),
            _ => Err(format!("Unknown model name: {}", model_name)),
        }
    }

    pub fn to_model_config(&self) -> ModelConfig {
        match self {
            // AIModel::MO3 => ModelConfig {
            //     model: "o3".to_string(),
            //     model_name: Some("observer".to_string()),
            //     top_p: todo!(),
            //     parallel_tool_calls: todo!(),
            //     temperature: todo!(),
            //     max_completion_tokens: todo!(),
            //     reasoning_effort: todo!(),
            //     presence_penalty: todo!(),
            //     strict: todo!(),
            // },
            AIModel::MO4Mini => ModelConfig {
                model: "o4-mini".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: None,
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: Some("low".to_string()),
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::MO4MiniDeepResearch => ModelConfig {
                model: "o4-mini-deep-research".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: None,
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: Some("low".to_string()),
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::MO3 => ModelConfig {
                model: "o3".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: None,
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: Some("low".to_string()),
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::M4dot1Nano => ModelConfig {
                model: "gpt-4.1-nano".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: None,
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::M4dot1Mini => ModelConfig {
                model: "gpt-4.1-mini".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: None,
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::M4dot1 => ModelConfig {
                model: "gpt-4.1".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: None,
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::Gemini2dot5Flash => ModelConfig {
                model: "gemini-2.5-flash".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: None,
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
        }
    }
}

impl Default for AIModel {
    fn default() -> Self {
        AIModel::MO4Mini
    }
}

impl ChannelState {
    pub async fn new(client: &Arc<OpenAIClient>) -> Self {
        // 新しい PromptStream を生成する
        let mut prompt_stream = client.create_prompt();
        prompt_stream.set_entry_limit(PROMPT_ENTRY_LIMIT).await;
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        Self {
            prompt_stream: Mutex::new(prompt_stream),
        }
    }

    async fn prepare_user_prompt(message: &mut InputMessage, viw_image_detail: u8) -> Vec<Message> {
        // スポイラーを含むメッセージの処理
        let re = Regex::new(r"(\|\|.*?\|\|)").unwrap();
        message.content = re
            .replace_all(&message.content, "||<spoiler_msg>||")
            .to_string();

        // メンションの処理（<@user_id> を @user_name に変換）
        let mention_re = Regex::new(r"<@(\d+)>").unwrap();
        message.content = mention_re
            .replace_all(&message.content, "@user")
            .to_string();

        // !hidetail が含まれていれば強制的に high detail
        let mut detail_flag = viw_image_detail;
        if message.content.contains("!hidetail") {
            println!("hidetail found in message content");
            detail_flag = 255;
            // 末尾／文中のフラグ文字列を削除
            message.content = message.content.replace("!hidetail", "");
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

    pub async fn reasoning(
        &self,
        ctx: &Context,
        msg: &serenity::all::Message,
        mut message: InputMessage,
        model: AIModel,
    ) -> String {
        // プロンプトストリームの取得
        let user_prompt = ChannelState::prepare_user_prompt(&mut message, 1).await;
        let mut r_prompt_stream = self.prompt_stream.lock().await;
        r_prompt_stream.add(user_prompt).await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream); // 先にロックを解除

        // モデル設定の確認とセット
        let model_config = model.to_model_config();
        debug!("Using model config: {:?}", model_config);
        prompt_stream.client.set_model_config(&model_config);
        prompt_stream.set_entry_limit(u64::MAX).await;
        let last_pos = prompt_stream.prompt.len();

        // システムプロンプトの追加
        debug!(
            "prompt_stream before system prompt - {:#?}",
            prompt_stream.prompt
        );
        let system_prompt = vec![Message::Developer {
            content: ASK_DEVELOPER_PROMPT.to_string(),
            name: Some(ASSISTANT_NAME.to_string()),
        }];
        prompt_stream.add_last(system_prompt).await;
        debug!(
            "prompt_stream after system prompt - {:#?}",
            prompt_stream.prompt
        );

        // 使用したツールのトラッキング
        let mut used_tools = Vec::new();

        // 推論ストリームの生成
        let mut reasoning_stream = match prompt_stream.reasoning(None, &ToolMode::Auto).await {
            Ok(stream) => stream,
            Err(e) => {
                debug!("Failed to create reasoning stream: {:?}", e);
                // フォールバック: ツールを無効にして再試行
                debug!("Attempting fallback with tools disabled");
                match prompt_stream.reasoning(None, &ToolMode::Disable).await {
                    Ok(stream) => {
                        debug!("Fallback reasoning stream created successfully");
                        stream
                    }
                    Err(fallback_e) => {
                        debug!("Fallback also failed: {:?}", fallback_e);
                        return format!(
                            "Err: failed reasoning (both auto and fallback) - Original: {:?}, Fallback: {:?}",
                            e, fallback_e
                        );
                    }
                }
            }
        };

        // 推論ループ
        for i in 0..*MAX_USE_TOOL_COUNT + 1 {
            // 終了できるなら終了
            if reasoning_stream.can_finish() {
                break;
            }

            // ツールコールの表示
            let show_tool_call: Vec<(String, serde_json::Value)> = reasoning_stream
                .show_tool_calls()
                .into_iter()
                .map(|(n, arg)| (n.to_string(), arg.clone()))
                .collect();

            info!("show_tool_call - {:#?}", show_tool_call);
            for (tool_name, argument) in show_tool_call {
                used_tools.push(tool_name.clone());
                if let Some(explain) = argument.get("$explain") {
                    let status_res = CreateMessage::new()
                        .content(format!("-# {}...", explain.to_string()))
                        .flags(MessageFlags::SUPPRESS_EMBEDS);

                    if let Err(e) = msg.channel_id.send_message(&ctx.http, status_res).await {
                        debug!("Error sending message: {:?}", e);
                    }
                } else {
                    let status_res = CreateMessage::new()
                        .content(format!("-# using {}...", tool_name))
                        .flags(MessageFlags::SUPPRESS_EMBEDS);

                    if let Err(e) = msg.channel_id.send_message(&ctx.http, status_res).await {
                        debug!("Error sending message: {:?}", e);
                    }
                }
            }

            // 推論の上限回数を超えた場合はツールモードを無効化
            let mode = if i == *MAX_USE_TOOL_COUNT {
                ToolMode::Disable
            } else {
                ToolMode::Auto
            };
            // 推論の実行
            match reasoning_stream.proceed(&mode).await {
                Err(e) => {
                    debug!("Failed to proceed reasoning: {:?}", e);
                    return format!("Err: failed reasoning proceed - {:?}", e);
                }
                Ok(_) => {
                    debug!(
                        "Reasoning proceeded. Current content: {:?}, Tool calls: {:#?}",
                        reasoning_stream.content,
                        reasoning_stream.show_tool_calls()
                    );
                }
            }
        }

        // 推論結果の取得
        let content = reasoning_stream
            .content
            .clone()
            .unwrap_or("Err: response is none from ai".to_string());
        debug!("Model output content: {:#?}", content);

        // ツールコールの統計収集
        let model_info = format!(
            "\n-# model: {}",
            prompt_stream.client.model_config.unwrap().model
        );
        let mut tool_count = HashMap::new();
        for tool in used_tools {
            *tool_count.entry(tool).or_insert(0) += 1;
        }
        let used_tools_info = if !tool_count.is_empty() {
            let tools_info: Vec<String> = tool_count
                .iter()
                .map(|(tool, count)| {
                    if *count > 1 {
                        format!("{} x{}", tool, count)
                    } else {
                        tool.clone()
                    }
                })
                .collect();
            format!("\n-# tools: {}", tools_info.join(", "))
        } else {
            "".to_string()
        };
        // プロンプトストリームに分岐した分部をマージ
        let differential_stream = prompt_stream.prompt.split_off(
            last_pos + 1, /* 先頭のシステムプロンプト消す */
        );
        {
            let mut r_prompt_stream = self.prompt_stream.lock().await;
            r_prompt_stream.add(differential_stream.into()).await;
        }
        return content.replace("\\n", "\n") + &model_info + &used_tools_info;
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
