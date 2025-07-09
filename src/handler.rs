use std::sync::Arc;

use crate::agent::{AIModel, ChannelState, InputMessage};
use call_agent::chat::client::OpenAIClient;
use dashmap::DashMap;
use log::{error, info, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serenity::{
    all::{
        ChannelId, Command, CommandOptionType, Context, CreateCommand, CreateCommandOption,
        CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
        EditInteractionResponse, Interaction, Message, MessageFlags, Ready, UserId,
    },
    async_trait,
    prelude::EventHandler,
};
use tokio::time;
use std::time::Duration;
use std::{str::FromStr, time::{SystemTime, UNIX_EPOCH}};

use observer::prefix::{ADMIN_USERS, RATE_CP, SEC_PER_RATE};

const TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChConf {
    pub enable: bool,
}

pub struct Handler {
    /// Handlerに1つのOpenAIClientを保持
    pub base_client: Arc<OpenAIClient>,
    /// 有効なチャンネルのset
    pub channels_conf: DashMap<u64, ChConf>,
    /// 各チャンネルごとの状態（会話履歴）を保持（DashMapは並列処理可能）
    pub channels: DashMap<ChannelId, Arc<ChannelState>>,
    /// ユーザーごとにレートリミット
    pub user_configs: DashMap<String, PerUserConfig>,
}

#[derive(Clone, Debug)]
pub struct PerUserConfig {
    pub rate_limit: u64, // レートリミットの秒数
    pub model: AIModel,
}

impl Default for PerUserConfig {
    fn default() -> Self {
        Self {
            rate_limit: 1,
            model: AIModel::default(),
        }
    }
}

impl Handler {
    /// チャンネルの状態を取得または作成する
    async fn get_or_create_channel_state(&self, channel_id: ChannelId) -> Arc<ChannelState> {
        if let Some(existing) = self.channels.get(&channel_id) {
            Arc::clone(&existing)
        } else {
            let new_state = Arc::new(ChannelState::new(&self.base_client).await);
            self.channels.insert(channel_id, new_state.clone());
            new_state
        }
    }

    /// メッセージを推論する
    async fn handle_mentioned_message(
        &self,
        ctx: &Context,
        msg: &Message,
        state: Arc<ChannelState>,
        message: InputMessage,
    ) -> String {
        // 有効なチャンネルかどうかを確認
        if let Some(conf) = self.channels_conf.get(&msg.channel_id.get()) {
            if !conf.enable {
                return "Err: AI is disabled in this channel".to_string();
            }
        } else {
            return "Err: AI is disabled in this channel".to_string();
        }

        // 使用モデルの取り出し
        let user_id = message.user_id.clone();
        let mut user_conf = self.user_configs.entry(user_id.clone()).or_insert_with(PerUserConfig::default);
        
        let model = user_conf.model.clone();
        let model_cost = model.to_sec_per_rate() as u64; // モデルのレート使用量
        let sec_per_rate = *SEC_PER_RATE as u64; // レートの回復時間
        let cp = *RATE_CP as u64; // レートの許容量
        
        // レートリミットの計算
        let limit_line = sec_per_rate * cp;
        let add_line = model_cost * sec_per_rate;
        let time_stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let mut user_line = user_conf.rate_limit;
        if user_line > time_stamp + limit_line {
            return format!("Err: rate limit - try again after <t:{}:R>", (user_line - limit_line));
        }
        if user_line == 0 {
            // リミットレスアカウント
        } else if user_line < time_stamp {
            user_line = time_stamp + add_line;
        } else {
            user_line += add_line;
        }
        user_conf.rate_limit = user_line;

        // タイピング表示のタスクを開始する
        let typing_task = tokio::spawn({
            let ctx = ctx.clone();
            let channel_id = msg.channel_id;
            async move {
                loop {
                    if let Err(e) = channel_id.broadcast_typing(&ctx.http).await {
                        error!("setting typing indicator - {:?}", e);
                    }
                    time::sleep(Duration::from_secs(4)).await;
                }
            }
        });

        // AIに質問、タイムアウトを設定
        let answer_text = match time::timeout(TIMEOUT, state.reasoning(ctx, msg, message, model)).await {
            Ok(answer) => answer,
            Err(_) => "Err: timeout".to_string(),
        };
        typing_task.abort();
        answer_text
    }

    /// メッセージを分割して送信する
    async fn send_split_message(&self, ctx: &Context, channel_id: ChannelId, text: String) {
        let chunks = Self::split_into_chunks(&text, 2000);

        // 最初のチャンクを送信
        if let Some(first_chunk) = chunks.get(0) {
            let response = CreateMessage::new()
                .content(first_chunk)
                .flags(MessageFlags::SUPPRESS_EMBEDS);
            if let Err(why) = channel_id.send_message(&ctx.http, response).await {
                error!("{:?}", why);
            }
        }

        // 残りのチャンクを送信
        for chunk in chunks.iter().skip(1) {
            let response = CreateMessage::new()
                .content(chunk)
                .flags(MessageFlags::SUPPRESS_EMBEDS);
            if let Err(why) = channel_id.send_message(&ctx.http, response).await {
                error!("{:?}", why);
            }
        }
    }

    /// テキストを指定された長さで分割する
    fn split_into_chunks(text: &str, max_len: usize) -> Vec<String> {
        // kaomoji の中のバッククォートだけをエスケープする
        let kaomoji_re = Regex::new(r"\([^)]+`[^)]+\)").unwrap();
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for line in text.lines() {
            let escaped = if kaomoji_re.is_match(line) {
                kaomoji_re
                    .replace_all(line, |caps: &regex::Captures| {
                        // マッチした kaomoji 部分だけバッククォートを \` に置換
                        caps[0].replace("`", r"\`")
                    })
                    .into_owned()
            } else {
                line.to_string()
            };

            if current_chunk.len() + escaped.len() + 1 > max_len {
                chunks.push(current_chunk);
                current_chunk = String::new();
            }
            if !current_chunk.is_empty() {
                current_chunk.push('\n');
            }
            current_chunk.push_str(&escaped);
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }
        chunks
    }

    /// チャンネル設定の保存
    fn save_ch_conf(&self) {
        let json_path = "./data/ch_conf.json";
        let mut conf_map = std::collections::HashMap::new();
        for entry in self.channels_conf.iter() {
            conf_map.insert(*entry.key(), entry.value().clone());
        }
        match serde_json::to_string_pretty(&conf_map) {
            Ok(json_str) => {
                if let Err(e) = std::fs::write(json_path, json_str) {
                    error!("Failed to write channel configuration to {}: {:?}", json_path, e);
                } else {
                    info!("Channel configuration saved to {}", json_path);
                }
            }
            Err(e) => {
                error!("Failed to serialize channel configuration: {:?}", e);
            }
        }
    }

    /// チャンネル設定の読み込み
    pub fn load(&self) {
        let json_path = "./data/ch_conf.json";
        if let Ok(json_str) = std::fs::read_to_string(json_path) {
            match serde_json::from_str::<std::collections::HashMap<u64, ChConf>>(&json_str) {
                Ok(conf_map) => {
                    for (key, value) in conf_map {
                        self.channels_conf.insert(key, value);
                    }
                    info!("Channel configuration loaded from {}", json_path);
                }
                Err(e) => {
                    error!("Failed to deserialize channel configuration: {:?}", e);
                }
            }
        } else {
            info!("No channel configuration found at {}", json_path);
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    /// メッセージが送信されたときの処理
    async fn message(&self, ctx: Context, msg: Message) {
        // Bot自身のメッセージは無視する
        let bot_id = ctx.cache.current_user().id;
        if msg.author.id == bot_id {
            return;
        }

        // 画像ファイル URL をフィルタして取得
        let attachment_urls: Vec<String> = msg
            .attachments
            .iter()
            .map(|att| att.url.clone())
            .collect();


        let state = self.get_or_create_channel_state(msg.channel_id).await;

        let message = InputMessage {
            content: msg.content.clone(),
            name: msg.author.name.clone(),
            message_id: msg.id.to_string(),
            reply_msg: msg.referenced_message.as_ref().map(|m| m.content.clone() + &m.attachments.iter().map(|att| att.url.clone()).collect::<Vec<String>>().join(", ")),
            user_id: msg.author.id.to_string(),
            attached_files: attachment_urls,
        };

        info!("Message: {:?}", message);

        let is_mentioned = msg.mentions.iter().any(|user| user.id == bot_id);

        if is_mentioned {
            let answer_text = self.handle_mentioned_message(&ctx, &msg, state, message).await;
            self.send_split_message(&ctx, msg.channel_id, answer_text).await;
        } else {
            state.add_message(message).await;
        }
    }

    
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            match command.data.name.as_str() {
                "ping" => {
                    let start = std::time::Instant::now();
                    let response_data = CreateInteractionResponseMessage::new()
                        .content("Pong!: Measuring latency...");
                    let response = CreateInteractionResponse::Message(response_data);
                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to ping - {:?}", why);
                        return;
                    }
                    let latency = start.elapsed().as_millis();
                    let edit = EditInteractionResponse::new()
                        .content(format!("Pong! latency: {} ms", latency));
                    
                    if let Err(why) = command.edit_response(&ctx.http, edit).await {
                        error!("Failed to edit ping response - {:?}", why);
                    }
                }

                "reset" => {
                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        existing.clone()
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };

                    state.clear_prompt().await;

                    let response_data = CreateInteractionResponseMessage::new()
                    .content("reset brain");

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to reset: {:?}", why);
                    }
                }

                "enable" => {
                    let channel_id = command.channel_id.get();
                    if let Some(mut ch_conf) = self.channels_conf.get_mut(&channel_id) {
                        if ch_conf.enable {
                            let response_data = CreateInteractionResponseMessage::new()
                            .content("Info: AI is already enabled");

                            let response = CreateInteractionResponse::Message(response_data);

                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to enable - {:?}", why);
                            }
                            return;
                        } else {
                            ch_conf.enable = true;

                            let response_data = CreateInteractionResponseMessage::new()
                            .content("Info: AI is enabled");

                            let response = CreateInteractionResponse::Message(response_data);

                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to enable - {:?}", why);
                            }
                            self.save_ch_conf();
                        }
                    } else {
                        self.channels_conf.insert(channel_id, ChConf { enable: true });
                        let response_data = CreateInteractionResponseMessage::new()
                        .content("Info: AI is enabled");
                        let response = CreateInteractionResponse::Message(response_data);
                        if let Err(why) = command.create_response(&ctx.http, response).await {
                            error!("Failed to respond to enable - {:?}", why);
                        }
                        self.save_ch_conf();
                    }
                }

                "disable" => {
                    let channel_id = command.channel_id.get();
                    if let Some(mut ch_conf) = self.channels_conf.get_mut(&channel_id) {
                        if !ch_conf.enable {
                            let response_data = CreateInteractionResponseMessage::new()
                            .content("Info: AI is already disabled");

                            let response = CreateInteractionResponse::Message(response_data);

                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to disable - {:?}", why);
                            }
                            return;
                        } else {
                            ch_conf.enable = false;

                            let response_data = CreateInteractionResponseMessage::new()
                            .content("Info: AI is disabled");

                            let response = CreateInteractionResponse::Message(response_data);

                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to disable - {:?}", why);
                            }
                            self.save_ch_conf();
                        }
                    } else {
                        self.channels_conf.insert(channel_id, ChConf { enable: false });
                        let response_data = CreateInteractionResponseMessage::new()
                        .content("Info: AI is disabled");
                        let response = CreateInteractionResponse::Message(response_data);
                        if let Err(why) = command.create_response(&ctx.http, response).await {
                            error!("Failed to respond to disable - {:?}", why);
                        }
                        self.save_ch_conf();
                    }
                }

                "collect_history" => {
                    let entry_num = command.data.options.get(0)
                        .and_then(|opt| opt.value.as_i64())
                        .map(|val| val as usize)
                        .unwrap_or(32);
                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        existing.clone()
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };
                    
                    use serenity::futures::StreamExt;
                    use std::pin::pin;
                    let mut messages_stream = pin!(command.channel_id.messages_iter(&ctx.http).take(entry_num));
                    let mut messages_vec = Vec::new();
                    while let Some(message_result) = messages_stream.next().await {
                        if let Ok(message) = message_result {
                            messages_vec.push(message);
                        }
                    }
                    for message in messages_vec.into_iter().rev() {
                        state.add_message(InputMessage {
                            content: message.content.clone(),
                            name: message.author.name.clone(),
                            message_id: message.id.to_string(),
                            reply_msg: message.referenced_message.as_ref().map(|m| m.content.clone()),
                            user_id: message.author.id.to_string(),
                            attached_files: Vec::new(),
                        }).await;
                    }
                    
                    let response_data = CreateInteractionResponseMessage::new()
                        .content(format!("Info: Complete collecting history ({} entries)", entry_num));

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to collect_history - {:?}", why);
                    }
                }

                "rate_conf" => {
                    let command_user_id = command.user.id.to_string();
                    if !ADMIN_USERS.contains(&command_user_id) {
                        let response_data = CreateInteractionResponseMessage::new()
                            .content("Error: You do not have permission to modify rate limits.");
                        let response = CreateInteractionResponse::Message(response_data);
                        if let Err(why) = command.create_response(&ctx.http, response).await {
                            error!("Failed to respond to rate_conf - {:?}", why);
                        }
                        return;
                    }
                    let user_line = if command.data.options.len() > 1 {
                        command.data.options[1].value.as_i64().unwrap_or(0) as i64
                    } else {
                        1
                    };
                    let target_user_id = match command.data.options[0].value.as_user_id() {
                        Some(user_id) => user_id.to_string(),
                        None => {
                            let response_data = CreateInteractionResponseMessage::new()
                                .content("Error: Invalid user ID.");
                            let response = CreateInteractionResponse::Message(response_data);
                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to rate_conf - {:?}", why);
                            }
                            return;
                        }
                    };
                    // ユーザーidから名前を取得
                    let user_data = UserId::from_str(&target_user_id).unwrap().to_user(&ctx.http).await.unwrap_or_default();
                    let target_user_name = user_data.name.clone();

                    let mut user_conf = self.user_configs.entry(target_user_id.clone()).or_insert(
                        PerUserConfig {
                            rate_limit: 0, // デフォルトは無制限
                            model: AIModel::default(), // デフォルトモデルを使用
                        }
                    );

                    // レートリミットを設定
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Time went backwards")
                        .as_secs();
                    if user_line == 0 {
                        user_conf.rate_limit = 0; // 無制限
                    } else if user_line < 0 {
                        user_conf.rate_limit = timestamp; // リセット
                    } else {
                        if user_conf.rate_limit < timestamp {
                            user_conf.rate_limit = timestamp;
                        }
                        let sec_per_rate = *SEC_PER_RATE as u64; 
                        user_conf.rate_limit += user_line as u64 * sec_per_rate;
                    }
                    let message = if user_line == 0 {
                        format!("Info: {} rate limit line set to unlimited", target_user_name).to_string()
                    } else {
                        let sec_per_rate = *SEC_PER_RATE as u64; // レートの回復時間
                        let cp = *RATE_CP as u64; // レートの許容量
                        
                        // レートリミットの計算
                        let limit_line = sec_per_rate * cp;
                        let now_rate = ((timestamp + limit_line) as i64 - user_conf.rate_limit as i64) / sec_per_rate as i64;
                        let next_time =  user_conf.rate_limit - limit_line;
                        format!("Info: rate limit forcibly consumed. Now {}'s rate is {} (relative: <t:{}:R>)", target_user_name, now_rate, next_time)
                    };
                    let response_data = CreateInteractionResponseMessage::new()
                        .content(message);

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to rate_conf - {:?}", why);
                    }
                }

                "model" => {
                    let command_user_id = command.user.id.to_string();
                    let default_model_name = AIModel::default().to_model_name();
                    let model_name = command.data.options[0].value.as_str().unwrap_or(&default_model_name);
                    let model = AIModel::from_model_name(model_name);
                    match model {
                        Err(e_str) => {
                            let response_data = CreateInteractionResponseMessage::new()
                                .content(format!("Error: {}", e_str))
                                .ephemeral(true);
                            let response = CreateInteractionResponse::Message(response_data);
                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to model - {:?}", why);
                            }
                            return;
                        },
                        Ok(model) => {
                            let mut user_conf = self.user_configs.entry(command_user_id.clone()).or_insert(
                                PerUserConfig {
                                    rate_limit: 0, // デフォルトは無制限
                                    model: AIModel::default(), // デフォルトモデルを使用
                                }
                            );
                            user_conf.model = model.clone();
                            let response_data = CreateInteractionResponseMessage::new()
                                .content(format!("Info: Model set to {}", model.to_model_name()))
                                .ephemeral(true);
    
                            let response = CreateInteractionResponse::Message(response_data);
        
                            if let Err(why) = command.create_response(&ctx.http, response).await {
                                error!("Failed to respond to model - {:?}", why);
                            }
                            return ;
                        }
                    }

                }


                _ => warn!("Unknown command: {}", command.data.name),
            }
        }
    }

    /// Bot が起動したときの処理
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        // グローバルコマンドを登録
        Command::set_global_commands(&ctx.http, vec![
            CreateCommand::new("ping")
                .description("Pong! 🏓"),
            CreateCommand::new("reset")
                .description("reset brain"),

            CreateCommand::new("enable")
                .description("enable AI"),

            CreateCommand::new("disable")
                .description("disable AI"),

            CreateCommand::new("collect_history")
                .description("collect message history")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "entry_num", "number of entries to collect")
                        .max_int_value(128)
                        .min_int_value(1)
                ),
            CreateCommand::new("rate_conf")
                .description("modify user rate")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::User, "user", "user to modify")
                        .required(true)
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "user_line", "0 for unlimited")
                        .required(true)
                        .add_int_choice("reset", -1)
                        .add_int_choice("Unlimited", 0)
                        .add_int_choice("sub 1", 1)
                        .add_int_choice("sub 2", 2)
                        .add_int_choice("sub 4", 4)
                        .add_int_choice("sub 8", 8)
                        .add_int_choice("sub 16", 16)
                        .add_int_choice("sub 32", 32)
                        .add_int_choice("sub 64", 64)
                        .add_int_choice("sub 128", 128)
                        .add_int_choice("sub 256", 256)
                        .add_int_choice("sub 512", 512)
                        .add_int_choice("sub 1024", 1024)
                        .add_int_choice("sub 2048", 2048)
                        .add_int_choice("sub 4096", 4096)
                        .add_int_choice("sub 8192", 8192)
                        .add_int_choice("sub 16384", 16384)
                        .add_int_choice("sub 32768", 32768)
                        .add_int_choice("sub 65536", 65536)

                ),
                CreateCommand::new("model")
                .description("set using model")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "model_name", "name of model to use")
                        .required(true)
                        .add_string_choice(AIModel::MO4Mini.to_model_discription(), AIModel::MO4Mini.to_model_name())
                        .add_string_choice(AIModel::MO4MiniDeepResearch.to_model_discription(), AIModel::MO4MiniDeepResearch.to_model_name())
                        .add_string_choice(AIModel::MO3.to_model_discription(), AIModel::MO3.to_model_name())
                        .add_string_choice(AIModel::M4dot1Nano.to_model_discription(), AIModel::M4dot1Nano.to_model_name())
                        .add_string_choice(AIModel::M4dot1Mini.to_model_discription(), AIModel::M4dot1Mini.to_model_name())
                        .add_string_choice(AIModel::M4dot1.to_model_discription(), AIModel::M4dot1.to_model_name())
                )
            ])
        .await
        .expect("Failed to create global command");
    }
}