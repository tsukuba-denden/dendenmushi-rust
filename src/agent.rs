use std::{collections::HashMap, sync::Arc, u64};

use call_agent::chat::{
    client::{ModelConfig, OpenAIClient, OpenAIClientState, ToolMode},
    prompt::{Message, MessageContext, MessageImage},
};
use log::{debug, info};
use observer::prefix::{
    ASK_DEVELOPER_PROMPT, ASSISTANT_NAME, MAX_USE_TOOL_COUNT, MODEL_GENERATE_MAX_TOKENS, MODEL_NAME,
};
use regex::Regex;
use serenity::all::{Context, CreateMessage, MessageFlags};
use tokio::sync::Mutex;

use crate::fetch_and_encode_images;

pub const PROMPT_ENTRY_LIMIT: u64 = 48; // プロンプトのエントリ数の上限

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
    /// 自動選択（指定順でモデルをフォールバック）Auto,
    MO4Mini,
    MO3,
    M5Nano,
    M5Mini,
    M5,
    Gemini3FlashPreview,
    Gemini25Pro,
    Gemini25Flash,
    Gemini25FlashLite,
    Gemini20FlashLite,
    Gemini15Flash,
    Gemini15Pro,
    GeminiFlashLatest,
    Gemma327bIt,
    Gemma312bIt,
    Gemma34bIt,
    Gemma31bIt,
}

impl AIModel {
    /// Auto モードで試すモデル順（先頭ほど優先）
    pub fn auto_fallback_order() -> Vec<AIModel> {
        vec![
            AIModel::Gemini3FlashPreview,
            AIModel::Gemini25Pro,
            AIModel::Gemini25Flash,
            AIModel::Gemini25FlashLite,
            AIModel::Gemini20FlashLite,
            AIModel::Gemma327bIt,
            AIModel::Gemma312bIt,
            AIModel::Gemma34bIt,
            AIModel::Gemma31bIt,
        ]
    }

    pub fn to_model_name(&self) -> String {
        match self {
            AIModel::Auto => "auto".to_string(),
            AIModel::MO4Mini => "o4-mini".to_string(),
            AIModel::MO3 => "o3".to_string(),
            AIModel::M5Nano => "gpt-5-nano".to_string(),
            AIModel::M5Mini => "gpt-5-mini".to_string(),
            AIModel::M5 => "gpt-5".to_string(),
            AIModel::Gemini3FlashPreview => "gemini-3-flash-preview".to_string(),
            AIModel::Gemini25Pro => "gemini-2.5-pro".to_string(),
            AIModel::Gemini25Flash => "gemini-2.5-flash".to_string(),
            AIModel::Gemini25FlashLite => "gemini-2.5-flash-lite".to_string(),
            AIModel::Gemini20FlashLite => "gemini-2.0-flash-lite".to_string(),
            AIModel::Gemini15Flash => "gemini-1.5-flash".to_string(),
            AIModel::Gemini15Pro => "gemini-1.5-pro".to_string(),
            AIModel::GeminiFlashLatest => "gemini-flash-latest".to_string(),
            AIModel::Gemma327bIt => "gemma-3-27b-it".to_string(),
            AIModel::Gemma312bIt => "gemma-3-12b-it".to_string(),
            AIModel::Gemma34bIt => "gemma-3-4b-it".to_string(),
            AIModel::Gemma31bIt => "gemma-3-1b-it".to_string(),
        }
    }

    pub fn to_model_discription(&self) -> String {
        match self {
            AIModel::Auto => {
                let order = AIModel::auto_fallback_order()
                    .into_iter()
                    .map(|m| m.to_model_name())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                format!("auto: {}", order)
            }
            AIModel::MO4Mini => "o4-mini: late=10 4いつもの 数学とコーディングに強い".to_string(),
            AIModel::MO3 => "o3: late=20　推論".to_string(),
            AIModel::M5Nano => "gpt-5-nano: late=2 超高速応答".to_string(),
            AIModel::M5Mini => "gpt-5-mini: late=5 高速応答".to_string(),
            AIModel::M5 => "gpt-5: late=20 一般".to_string(),
            AIModel::Gemini3FlashPreview => {
                "gemini-3-flash-preview: Google Gemini 3 Flash (Preview)".to_string()
            }
            AIModel::Gemini25Pro => "gemini-2.5-pro: Google Gemini 高性能推論".to_string(),
            AIModel::Gemini25Flash => {
                "gemini-2.5-flash: Google Gemini 高速・汎用 (Vision/Tool対応)".to_string()
            }
            AIModel::Gemini25FlashLite => {
                "gemini-2.5-flash-lite: Google Gemini 超軽量・高速".to_string()
            }
            AIModel::Gemini20FlashLite => {
                "gemini-2.0-flash-lite: Google Gemini 低コスト・軽量".to_string()
            }
            AIModel::Gemini15Flash => {
                "gemini-1.5-flash: 高速マルチモーダル (Vision向け)".to_string()
            }
            AIModel::Gemini15Pro => "gemini-1.5-pro: 高性能推論".to_string(),
            AIModel::GeminiFlashLatest => {
                "gemini-flash-latest: 常に最新のFlash系 (将来の2.x/3.x系を自動追随)".to_string()
            }
            AIModel::Gemma327bIt => "gemma-3-27b-it: Google Gemma 3 27B IT".to_string(),
            AIModel::Gemma312bIt => "gemma-3-12b-it: Google Gemma 3 12B IT".to_string(),
            AIModel::Gemma34bIt => "gemma-3-4b-it: Google Gemma 3 4B IT".to_string(),
            AIModel::Gemma31bIt => "gemma-3-1b-it: Google Gemma 3 1B IT".to_string(),
        }
    }

    pub fn to_sec_per_rate(&self) -> usize {
        match self {
            AIModel::Auto => 5,
            AIModel::MO4Mini => 10,
            AIModel::MO3 => 20,
            AIModel::M5Nano => 2,
            AIModel::M5Mini => 5,
            AIModel::M5 => 20,
            AIModel::Gemini3FlashPreview => 5,
            AIModel::Gemini25Pro => 20,
            AIModel::Gemini25Flash => 5,
            AIModel::Gemini25FlashLite => 3,
            AIModel::Gemini20FlashLite => 3,
            AIModel::Gemini15Flash => 5,
            AIModel::Gemini15Pro => 20,
            AIModel::GeminiFlashLatest => 5,
            AIModel::Gemma327bIt => 20,
            AIModel::Gemma312bIt => 12,
            AIModel::Gemma34bIt => 4,
            AIModel::Gemma31bIt => 2,
        }
    }

    pub fn from_model_name(model_name: &str) -> Result<Self, String> {
        match model_name {
            // "o3" => Ok(AIModel::MO3),
            "auto" => Ok(AIModel::Auto),
            "o4-mini" => Ok(AIModel::MO4Mini),
            "o3" => Ok(AIModel::MO3),
            "gpt-5-nano" => Ok(AIModel::M5Nano),
            "gpt-5-mini" => Ok(AIModel::M5Mini),
            "gpt-5" => Ok(AIModel::M5),
            "gemini-3-flash-preview" => Ok(AIModel::Gemini3FlashPreview),
            "gemini-2.5-pro" => Ok(AIModel::Gemini25Pro),
            "gemini-2.5-flash" => Ok(AIModel::Gemini25Flash),
            "gemini-2.5-flash-lite" => Ok(AIModel::Gemini25FlashLite),
            "gemini-2.0-flash-lite" => Ok(AIModel::Gemini20FlashLite),
            "gemini-1.5-flash" => Ok(AIModel::Gemini15Flash),
            "gemini-1.5-pro" => Ok(AIModel::Gemini15Pro),
            "gemini-flash-latest" => Ok(AIModel::GeminiFlashLatest),
            "gemma-3-27b-it" => Ok(AIModel::Gemma327bIt),
            "gemma-3-12b-it" => Ok(AIModel::Gemma312bIt),
            "gemma-3-4b-it" => Ok(AIModel::Gemma34bIt),
            "gemma-3-1b-it" => Ok(AIModel::Gemma31bIt),
            _ => Err(format!("Unknown model name: {}", model_name)),
        }
    }

    pub fn to_model_config(&self) -> ModelConfig {
        match self {
            AIModel::Auto => {
                // Auto 自体は実モデルではないため、ここには来ない想定。
                // 念のため最優先モデルの設定を返す。
                AIModel::auto_fallback_order()
                    .into_iter()
                    .next()
                    .unwrap_or(AIModel::Gemini25Flash)
                    .to_model_config()
            }
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
            AIModel::M5Nano => ModelConfig {
                model: "gpt-5-nano".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: Some("low".to_string()),
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::M5Mini => ModelConfig {
                model: "gpt-5-mini".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: Some("low".to_string()),
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::M5 => ModelConfig {
                model: "gpt-5".to_string(),
                model_name: Some(ASSISTANT_NAME.to_string()),
                parallel_tool_calls: Some(true),
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: Some("low".to_string()),
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
                web_search_options: None,
            },
            AIModel::Gemini3FlashPreview => ModelConfig {
                model: "gemini-3-flash-preview".to_string(),
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
            AIModel::Gemini25Pro => ModelConfig {
                model: "gemini-2.5-pro".to_string(),
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
            AIModel::Gemini25Flash => ModelConfig {
                model: "gemini-2.5-flash".to_string(),
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
            AIModel::Gemini25FlashLite => ModelConfig {
                model: "gemini-2.5-flash-lite".to_string(),
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
            AIModel::Gemini20FlashLite => ModelConfig {
                model: "gemini-2.0-flash-lite".to_string(),
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
            AIModel::Gemini15Flash => ModelConfig {
                model: "gemini-1.5-flash".to_string(),
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
            AIModel::Gemini15Pro => ModelConfig {
                model: "gemini-1.5-pro".to_string(),
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
            AIModel::GeminiFlashLatest => ModelConfig {
                model: "gemini-flash-latest".to_string(),
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
            AIModel::Gemma327bIt => ModelConfig {
                model: "gemma-3-27b-it".to_string(),
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
            AIModel::Gemma312bIt => ModelConfig {
                model: "gemma-3-12b-it".to_string(),
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
            AIModel::Gemma34bIt => ModelConfig {
                model: "gemma-3-4b-it".to_string(),
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
            AIModel::Gemma31bIt => ModelConfig {
                model: "gemma-3-1b-it".to_string(),
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
        }
    }
}

impl Default for AIModel {
    fn default() -> Self {
        // 既存のデフォルト (gemini-flash-latest) は Auto に置き換え、
        // 指定順でフォールバックできるようにする。
        if *MODEL_NAME == "gemini-flash-latest" {
            return AIModel::Auto;
        }
        AIModel::from_model_name(*MODEL_NAME).unwrap_or(AIModel::Auto)
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
        // message は現状この関数内では未使用（将来拡張用）
        let _ = &mut message;

        // ベースとなるプロンプトストリームを取得
        let r_prompt_stream = self.prompt_stream.lock().await;
        let base_prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream);

        // Auto の場合は指定順でモデルを試す
        let is_auto = matches!(model, AIModel::Auto);
        let candidates = if is_auto {
            AIModel::auto_fallback_order()
        } else {
            vec![model.clone()]
        };

        let system_prompt = vec![Message::Developer {
            content: ASK_DEVELOPER_PROMPT.to_string(),
            name: Some(ASSISTANT_NAME.to_string()),
        }];

        let mut last_error: Option<String> = None;

        for candidate in candidates {
            let mut prompt_stream = base_prompt_stream.clone();
            let model_config = candidate.to_model_config();
            debug!("Using model config: {:?}", model_config);
            prompt_stream.client.set_model_config(&model_config);
            prompt_stream.set_entry_limit(u64::MAX).await;
            let last_pos = prompt_stream.prompt.len();

            // システムプロンプトの追加
            prompt_stream.add_last(system_prompt.clone()).await;

            // 使用したツールのトラッキング
            let mut used_tools = Vec::new();

            // 推論ストリームの生成（ツール無効フォールバックは維持）
            let mut reasoning_stream = match prompt_stream.reasoning(None, &ToolMode::Auto).await {
                Ok(stream) => stream,
                Err(e) => {
                    debug!("Failed to create reasoning stream: {:?}", e);
                    debug!("Attempting fallback with tools disabled");
                    match prompt_stream.reasoning(None, &ToolMode::Disable).await {
                        Ok(stream) => stream,
                        Err(fallback_e) => {
                            last_error = Some(format!(
                                "failed create stream - Original: {:?}, Fallback: {:?}",
                                e, fallback_e
                            ));
                            if is_auto {
                                continue;
                            }
                            return format!(
                                "Err: failed reasoning (both auto and fallback) - Original: {:?}, Fallback: {:?}",
                                e, fallback_e
                            );
                        }
                    }
                }
            };

            // 推論ループ
            let mut proceed_failed: Option<String> = None;
            for i in 0..*MAX_USE_TOOL_COUNT + 1 {
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

                match reasoning_stream.proceed(&mode).await {
                    Err(e) => {
                        debug!("Failed to proceed reasoning: {:?}", e);
                        proceed_failed = Some(format!("{:?}", e));
                        break;
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

            if let Some(e) = proceed_failed {
                last_error = Some(format!("failed proceed - {}", e));
                if is_auto {
                    continue;
                }
                return format!("Err: failed reasoning proceed - {}", e);
            }

            // 推論結果の取得
            let content = reasoning_stream
                .content
                .clone()
                .unwrap_or("Err: response is none from ai".to_string());

            // ツールコールの統計収集
            let used_model = prompt_stream
                .client
                .model_config
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| candidate.to_model_name());
            let model_info = if is_auto {
                format!("\n-# model: {}(Auto)", used_model)
            } else {
                format!("\n-# model: {}", used_model)
            };

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

            // プロンプトストリームに分岐した部分をマージ（先頭のシステムプロンプトは消す）
            let differential_stream = prompt_stream.prompt.split_off(
                last_pos + 1, /* 先頭のシステムプロンプト消す */
            );
            {
                let mut r_prompt_stream = self.prompt_stream.lock().await;
                r_prompt_stream.add(differential_stream.into()).await;
            }
            return content.replace("\\n", "\n") + &model_info + &used_tools_info;
        }

        let attempted = if is_auto {
            AIModel::auto_fallback_order()
                .into_iter()
                .map(|m| m.to_model_name())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            model.to_model_name()
        };

        format!(
            "Err: all model fallbacks failed. attempted=[{}], last_error={}",
            attempted,
            last_error.unwrap_or_else(|| "none".to_string())
        )
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
