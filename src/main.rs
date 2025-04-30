use std::{collections::HashSet, sync::Arc};
use agent::{ChannelState, InputMessage};
use dashmap::DashMap;
mod agent;

use call_agent::chat::client::{ModelConfig, OpenAIClient};
use observer::{prefix::{ADMIN_USERS, ASSISTANT_NAME, DISCORD_TOKEN, ENABLE_BROWSER_TOOL, ENABLE_GET_TIME_TOOL, ENABLE_IMAGE_CAPTIONER_TOOL, ENABLE_MEMORY_TOOL, ENABLE_WEB_DEPLOY_TOOL, MAIN_MODEL_API_KEY, MAIN_MODEL_ENDPOINT, MODEL_GENERATE_MAX_TOKENS, MODEL_NAME, RATE_CP, SEC_PER_RATE}, tools::{self, browsing_worker::BrowsingWorker, get_time::GetTime, image_captioner::ImageCaptionerTool, web_deploy::WebDeploy, web_scraper::Browser}};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use tools::memory::MemoryTool;

use serenity::{all::{CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse}, async_trait, futures::{self}};
use serenity::model::gateway::Ready;
use serenity::model::prelude::*;
use serenity::prelude::*;
use futures::StreamExt;
use std::time::Duration;
use log::{error, info, warn};
use regex::Regex;

use reqwest::Client as ReqwestClient;
use std::io::Cursor;
use image::{codecs::gif::GifDecoder, io::Reader as ImageReader, AnimationDecoder, DynamicImage, GenericImageView, RgbaImage};
use base64;

async fn fetch_and_encode_images(urls: &[String]) -> Vec<String> {
    println!("fetch_and_encode_images: {:?}", urls);
    // 拡張子チェック＆クエリ対応
    let ext_re = Regex::new(r"(?i)\.(png|jpe?g|gif|webp)(?:[?#].*)?$").unwrap();
    // パラメータなし画像URLを即取得する正規表現
    let strict_ext_re = Regex::new(r"(?i)\.(png|jpe?g|gif|webp)$").unwrap();
    let client = ReqwestClient::new();
    let mut total_bytes = 0u64;
    let mut out = Vec::new();

    for url in urls.iter().filter(|u| ext_re.is_match(u)) {
        // パラメータなし URL は問答無用でオリジナルを取得
        if strict_ext_re.is_match(url) {
            if let Ok(resp) = client.get(url).send().await {
                if let Ok(bytes) = resp.bytes().await {
                    // 拡張子から MIME を決定
                    let ext = strict_ext_re
                        .captures(url)
                        .and_then(|c| c.get(1))
                        .unwrap()
                        .as_str()
                        .to_lowercase();
                    let mime = match ext.as_str() {
                        "png"  => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif"  => "image/gif",
                        "webp" => "image/webp",
                        _      => "application/octet-stream",
                    };
                    out.push(format!("data:{};base64,{}", mime, base64::encode(&bytes)));
                }
            }
            continue;
        }
        let ext = ext_re.captures(url).and_then(|c| c.get(1)).unwrap().as_str().to_lowercase();
        // HEAD でサイズチェック
        let len = client.head(url).send().await
            .ok()
            .and_then(|r| r.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse().ok()))
            .unwrap_or(0);
        if len == 0 || len > 20 * 1024 * 1024 || total_bytes + len > 50 * 1024 * 1024 {
            continue;
        }
        // GET してバイト列取得
        let bytes = match client.get(url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        // 解像度チェック
        let reader = match ext.as_str() {
            "gif" => {
                let decoder = GifDecoder::new(Cursor::new(&bytes)).unwrap();
                let mut frames = decoder.into_frames();
        
                // Frame を取り出し
                let frame = match frames.next() {
                    Some(Ok(frame)) => frame,
                    _ => continue,
                };
        
                // Frame をバッファ（RgbaImage）に変換
                let buf: RgbaImage = frame.into_buffer();
                DynamicImage::ImageRgba8(buf)
            }
            _ => {
                // 通常の画像
                let img = match ImageReader::new(Cursor::new(&bytes)).with_guessed_format() {
                    Ok(reader) => match reader.decode() {
                        Ok(i) => i,
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                // 透過があれば白背景でフラット化
                if img.color().has_alpha() {
                    let (w, h) = img.dimensions();
                    let mut bg = RgbaImage::new(w, h);
                    for (x, y, p) in img.to_rgba8().enumerate_pixels() {
                        let alpha = p.0[3] as f32 / 255.0;
                        let inv = 1.0 - alpha;
                        let r = (p[0] as f32 * alpha + 255.0 * inv) as u8;
                        let g = (p[1] as f32 * alpha + 255.0 * inv) as u8;
                        let b = (p[2] as f32 * alpha + 255.0 * inv) as u8;
                        bg.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                    }
                    DynamicImage::ImageRgba8(bg)
                } else {
                    img
                }
            }
        };
        // 解像度を調整（長辺>2000なら縮小、短辺<512なら拡大）
        let (w, h) = reader.dimensions();
        let mut img = reader;
        // 長辺が2000pxを超える場合は縮小
        if img.dimensions().0.max(img.dimensions().1) > 2000 {
            let long = img.dimensions().0.max(img.dimensions().1) as f32;
            let scale = 2000.0 / long;
            img = img.resize(
                (w as f32 * scale) as u32,
                (h as f32 * scale) as u32,
                image::imageops::FilterType::Lanczos3,
            );
        }
        // 短辺が512px未満の場合は拡大
        if img.dimensions().0.min(img.dimensions().1) < 512 {
            let (w2, h2) = img.dimensions();
            let short = w2.min(h2) as f32;
            let scale = 512.0 / short;
            img = img.resize(
                (w2 as f32 * scale) as u32,
                (h2 as f32 * scale) as u32,
                image::imageops::FilterType::Lanczos3,
            );
        }
        // PNGで再エンコード → data URL
        let mut buf = Vec::new();
        if img
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .is_err()
        {
            continue;
        }
        total_bytes += len;
        out.push(format!("data:image/png;base64,{}", base64::encode(&buf)));
    }

    out
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChConf {
    enable: bool,
}

struct Handler {
    // Handlerに1つのOpenAIClientを保持
    base_client: Arc<OpenAIClient>,
    // 有効なチャンネルのset
    channels_conf: DashMap<u64, ChConf>,
    // 各チャンネルごとの状態（会話履歴）を保持（DashMapは並列処理可能）
    channels: DashMap<ChannelId, Arc<ChannelState>>,
    // ユーザーごとにレートリミット
    per_user_rate_limit: DashMap<String, u64>,
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

    // メッセージを推論する
    async fn handle_mentioned_message(
        &self,
        ctx: &Context,
        msg: &serenity::all::Message,
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
        let sec_per_rate = *SEC_PER_RATE as u64;
        let cp = *RATE_CP as u64;
        
        // レートリミットの計算
        let limit_line = sec_per_rate * cp;
        let user_id = message.user_id.clone();
        let time_stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let mut user_line = self.per_user_rate_limit.entry(user_id.clone()).or_insert(0);
        if *user_line > time_stamp + limit_line {
            return format!("Err: rate limit - try again after {} seconds", (*user_line - (time_stamp + limit_line)));
        }
        if *user_line == 0 {
            // リミットレスアカウント
        } else if *user_line < time_stamp {
            *user_line = time_stamp + sec_per_rate;
        } else {
            *user_line += sec_per_rate;
        }

        // タイピング表示のタスクを開始する
        let typing_task = tokio::spawn({
            let ctx = ctx.clone();
            let channel_id = msg.channel_id;
            async move {
                loop {
                    if let Err(e) = channel_id.broadcast_typing(&ctx.http).await {
                        error!("setting typing indicator - {:?}", e);
                    }
                    tokio::time::sleep(Duration::from_secs(4)).await;
                }
            }
        });

        // AIに質問、タイムアウトを設定
        let answer_text = match tokio::time::timeout(Duration::from_secs(180), state.reasoning(ctx, msg, message)).await {
            Ok(answer) => answer,
            Err(_) => "Err: timeout".to_string(),
        };
        typing_task.abort();
        answer_text
    }

    // メッセージを分割して送信する
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

    // テキストを指定された長さで分割する
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

    // チャンネル設定の保存
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

    fn load(&self) {
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
    async fn message(&self, ctx: Context, msg: serenity::all::Message) {
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
                    let response_data = CreateInteractionResponseMessage::new()
                    .content("Pong! 🏓");

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to ping - {:?}", why);
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

                "ask" => {
                    // 考え中
                    let defer_response = CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new()
                    );
                    if let Err(why) = command.create_response(&ctx.http, defer_response).await {
                        error!("Failed to send Defer response - {:?}", why);
                        return;
                    }

                    let question = command.data.options[0].value.as_str().unwrap();
                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        Arc::clone(&existing)
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };

                    let message = InputMessage {
                        content: question.to_string(),
                        name: command.user.name.clone(),
                        message_id: "".to_string(),
                        reply_msg: None,
                        user_id: command.user.id.to_string(),
                        attached_files: Vec::new(),
                    };

                    let answer_text = state.ask(message).await;

                    // 改行単位で分割し、2000文字を超えないようにする
                    let mut chunks = Vec::new();
                    let mut current_chunk = String::new();

                    for line in answer_text.lines() {
                        if current_chunk.len() + line.len() + 1 > 2000 {
                            chunks.push(current_chunk);
                            current_chunk = String::new();
                        }
                        if !current_chunk.is_empty() {
                            current_chunk.push('\n');
                        }
                        current_chunk.push_str(line);
                    }
                    if !current_chunk.is_empty() {
                        chunks.push(current_chunk);
                    }

                    // 最初のメッセージは `edit_response`
                    if let Some(first_chunk) = chunks.get(0) {
                        let response = EditInteractionResponse::new().content(first_chunk);
                        if let Err(why) = command.edit_response(&ctx.http, response).await {
                            error!("Failed to edit response - {:?}", why);
                        }
                    }

                    // 残りのメッセージは `followup_message`
                    for chunk in &chunks[1..] {
                        if let Err(why) = command
                            .create_followup(&ctx.http, CreateInteractionResponseFollowup::new().content(chunk).flags(MessageFlags::SUPPRESS_EMBEDS))
                            .await
                        {
                            error!("Failed to send follow-up message - {:?}", why);
                        }
                    }
                }

                "collect_history" => {
                    let entry_num = command.data.options[0].value.as_i64().unwrap_or(32) as usize;
                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        existing.clone()
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };
                    let mut messages_stream = Box::pin(command.channel_id.messages_iter(&ctx.http).take(entry_num));
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
                    let user_id = command.user.id.to_string();
                    if !ADMIN_USERS.contains(&user_id) {
                        let response_data = CreateInteractionResponseMessage::new()
                            .content("Error: You do not have permission to modify rate limits.");
                        let response = CreateInteractionResponse::Message(response_data);
                        if let Err(why) = command.create_response(&ctx.http, response).await {
                            error!("Failed to respond to rate_conf - {:?}", why);
                        }
                        return;
                    }
                    let user_line = if command.data.options.len() > 1 {
                        command.data.options[1].value.as_i64().unwrap_or(0) as u64
                    } else {
                        1
                    };
                    self.per_user_rate_limit.insert(user_id.clone(), user_line);
                    let response_data = CreateInteractionResponseMessage::new()
                        .content(format!("Info: rate limit set to {} for user {}", user_line, user_id));

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to rate_conf - {:?}", why);
                    }
                }


                _ => warn!("Unknown command: {}", command.data.name),
            }
        }
    }

    /// Bot が起動したときの処理
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
        tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let mut reader = tokio::io::BufReader::new(stdin).lines();

            while let Ok(Some(line)) = reader.next_line().await {
                if line == "exit" {
                    break;
                }

                let message = InputMessage {
                    content: line,
                    name: "root".to_string(),
                    message_id: "Null".to_string(),
                    reply_msg: None,
                    user_id: "Null".to_string(),
                    attached_files: Vec::new(),
                };

                let rs = new_state.ask(message).await;
                info!("AI:\n{}\n\n", rs);
            }
        });

        // グローバルコマンドを登録
        Command::set_global_commands(&ctx.http, vec![
            CreateCommand::new("ping")
                .description("Pong! 🏓"),
            CreateCommand::new("ask")
                .description(format!("ask {}", *ASSISTANT_NAME))
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "content", "question to ask")
                        .required(true)
                ),
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
                        .required(false)
                        .min_int_value(0)
                )
            ])
        .await
        .expect("Failed to create global command");
    }
}

#[tokio::main]
async fn main() {
    // ロガーの初期化
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .filter_module("serenity", log::LevelFilter::Off) // serenityクレートのログを除外
        .filter_module("reqwest", log::LevelFilter::Off) // reqwestクレートのログを除外
        .filter_module("hyper", log::LevelFilter::Off) // hyperクレートのログを除外
        .filter_module("rustls", log::LevelFilter::Off) // rustlsクレートのログを除外
        .filter_module("h2", log::LevelFilter::Off) // h2クレートのログを除外
        .filter_module("tungstenite", log::LevelFilter::Off) // tungsteniteクレートのログを除外
        .filter_module("tracing", log::LevelFilter::Off) // tracingクレートのログを除外
        .filter_module("html5ever", log::LevelFilter::Off) // html5everクレートのログを除外
        .filter_module("selectors", log::LevelFilter::Off) // selectorsクレートのログを除外
        .filter_module("playwright", log::LevelFilter::Off) // markup5everクレートのログを除外
        .init();

    // Discord Bot のトークンを取得
    let token = *DISCORD_TOKEN;

    // モデル設定
    let conf = ModelConfig {
        model: MODEL_NAME.to_string(),
        model_name: Some(ASSISTANT_NAME.to_string()),
        parallel_tool_calls: None,
        temperature: None,
        max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
        reasoning_effort: Some("low".to_string()),
        presence_penalty: None,
        strict: Some(false),
        top_p: Some(1.0),
    };

    // 基本となる OpenAIClient を生成し、ツールを定義
    let mut base_client = OpenAIClient::new(
        *MAIN_MODEL_ENDPOINT,
        Some(*MAIN_MODEL_API_KEY),
    );


    if *ENABLE_BROWSER_TOOL {
        base_client.def_tool(Arc::new(Browser::new()));
    }
    if *ENABLE_MEMORY_TOOL {
        base_client.def_tool(Arc::new(MemoryTool::new()));
    }
    if *ENABLE_GET_TIME_TOOL {
        base_client.def_tool(Arc::new(GetTime::new()));
    }
    if *ENABLE_WEB_DEPLOY_TOOL {
        let web_deploy = Arc::new(WebDeploy::new().await);
        web_deploy.start_server("0.0.0.0:80".to_string());
        base_client.def_tool(web_deploy);
    }
    if *ENABLE_IMAGE_CAPTIONER_TOOL {
        base_client.def_tool(Arc::new(
            ImageCaptionerTool::new({

                let mut c = OpenAIClient::new(
                    *MAIN_MODEL_ENDPOINT,
                    Some(*MAIN_MODEL_API_KEY)
                );
                c.set_model_config(&ModelConfig {
                    model: "gpt-4.1-nano".to_string(),
                    model_name: Some("image_captioner".to_string()),
                    parallel_tool_calls: None,
                    temperature: None,
                    max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                    reasoning_effort: None,
                    presence_penalty: None,
                    strict: Some(false),
                    top_p: Some(1.0),
                });
                c
            })
        ));
    }
    base_client.def_tool(Arc::new(
        BrowsingWorker::new({
            let mut c = OpenAIClient::new(
                *MAIN_MODEL_ENDPOINT,
                Some(*MAIN_MODEL_API_KEY)
            );
            c.set_model_config(&ModelConfig {
                model: "gpt-4.1-nano".to_string(),
                model_name: Some("browsing_worker".to_string()),
                parallel_tool_calls: None,
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: None,
                presence_penalty: None,
                strict: Some(false),
                top_p: Some(1.0),
            });
            c
        })
        )
    );
    base_client.set_model_config(&conf);
    let base_client = Arc::new(base_client);

    let channels = DashMap::new();


    // Bot のインテント設定（MESSAGE_CONTENT を含む）
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let handler = Handler {
        base_client: base_client.clone(),
        channels: channels.clone(),
        channels_conf: DashMap::new(),
        per_user_rate_limit: DashMap::new(),
    };
    handler.load();
    let mut client = Client::builder(&token, intents)
        .event_handler(handler)
        .await
        .expect("Error creating client");

    if let Err(e) = client.start().await {
        error!("Client error: {:?}", e);
    }
}