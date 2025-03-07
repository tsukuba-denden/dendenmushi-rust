use std::sync::Arc;
use agent::{ChannelState, InputMessage};
use dashmap::DashMap;
mod agent;

use call_agent::chat::client::{ModelConfig, OpenAIClient};
use observer::{prefix::{ASSISTANT_NAME, DISCORD_TOKEN, MAIN_MODEL_API_KEY, MAIN_MODEL_ENDPOINT, MODEL_GENERATE_MAX_TOKENS, MODEL_NAME}, tools::{self, get_time::GetTime, web_deploy::WebDeploy}};
use tokio::io::AsyncBufReadExt;
use tools::{memory::MemoryTool, web_scraper::WebScraper};

use serenity::{all::{CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse}, async_trait, futures::{self}};
use serenity::model::gateway::Ready;
use serenity::model::prelude::*;
use serenity::prelude::*;
use futures::StreamExt;
use std::time::Duration;
use log::{error, info, warn};

struct Handler {
    // Handlerに1つのOpenAIClientを保持
    base_client: Arc<OpenAIClient>,
    // 各チャンネルごとの状態（会話履歴）を保持（DashMapは並列処理可能）
    channels: DashMap<ChannelId, Arc<ChannelState>>,
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

        let state = if let Some(existing) = self.channels.get(&msg.channel_id) {
            Arc::clone(&existing)
        } else {
            let new_state = Arc::new(ChannelState::new(&self.base_client).await);
            self.channels.insert(msg.channel_id, new_state.clone());
            new_state
        };

        let message = InputMessage {
            content: msg.content,
            name: msg.author.name.clone(),
            message_id: msg.id.to_string(),
            reply_to: msg.referenced_message.as_ref().map(|m| m.id.to_string()),
            user_id: msg.author.id.to_string(),
        };

        info!("Message: {:?}", message.clone());

        // このメッセージがBotにメンションされているかどうかを確認
        let is_mentioned = msg.mentions.iter().any(|user| user.id == bot_id);

        // Botにメンションされている場合はAIに質問し、そうでない場合は会話履歴に追加
        if is_mentioned {
            // タイピング表示
            let typing_task = tokio::spawn({
                let ctx = ctx.clone();
                let channel_id = msg.channel_id;
                async move {
                    loop {
                        if let Err(e) = channel_id.broadcast_typing(&ctx.http).await {
                            error!("setting typing indicator - {:?}", e);
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                    }
                }
            });
            let answer_text = match tokio::time::timeout(Duration::from_secs(120), state.ask(message)).await {
                Ok(answer) => answer,
                Err(_) => "Err: timeout".to_string(),
            };
            typing_task.abort();

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

            // 最初のメッセージを送信
            if let Some(first_chunk) = chunks.get(0) {
                let response = CreateMessage::new()
                    .content(first_chunk)
                    .flags(MessageFlags::SUPPRESS_EMBEDS);
                if let Err(why) = msg.channel_id.send_message(&ctx.http, response).await {
                    error!("{:?}", why);
                }
            }

            // 残りのメッセージを送信
            for chunk in &chunks[1..] {
                let response = CreateMessage::new()
                    .content(chunk)
                    .flags(MessageFlags::SUPPRESS_EMBEDS);
                if let Err(why) = msg.channel_id.send_message(&ctx.http, response).await {
                    error!("{:?}", why);
                }
            }
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
                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        existing.clone()
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };

                    state.enable().await;

                    let response_data = CreateInteractionResponseMessage::new()
                    .content("Info: Enabled AI");

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to enable - {:?}", why);
                    }
                }

                "disable" => {
                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        existing.clone()
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };

                    state.disable().await;

                    let response_data = CreateInteractionResponseMessage::new()
                    .content("Info: Disabled AI");

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to disable - {:?}", why);
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
                        reply_to: None,
                        user_id: command.user.id.to_string(),
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


                "deep_search" => {
                    // 考え中
                    let defer_response = CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new()
                    );
                    if let Err(why) = command.create_response(&ctx.http, defer_response).await {
                        error!("Failed to send Defer response - {:?}", why);
                        return;
                    }

                    let question = command.data.options[0].value.as_str().unwrap();
                    let try_count = if command.data.options.len() > 1 {
                        command.data.options[1].value.as_i64().unwrap_or(10) as usize
                    } else {
                        10
                    };

                    let state = if let Some(existing) = self.channels.get(&command.channel_id) {
                        existing.clone()
                    } else {
                        let new_state = Arc::new(ChannelState::new(&self.base_client).await);
                        self.channels.insert(command.channel_id, new_state.clone());
                        new_state
                    };

                    let message = InputMessage {
                        content: question.to_string(),
                        name: command.user.name.clone(),
                        message_id: "".to_string(),
                        reply_to: None,
                        user_id: command.user.id.to_string(),
                    };

                    let answer_text = state.deep_search(message, try_count).await;

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
                            reply_to: message.referenced_message.as_ref().map(|m| m.id.to_string()),
                            user_id: message.author.id.to_string(),
                        }).await;
                    }
                    
                    let response_data = CreateInteractionResponseMessage::new()
                        .content(format!("Info: Complete collecting history ({} entries)", entry_num));

                    let response = CreateInteractionResponse::Message(response_data);

                    if let Err(why) = command.create_response(&ctx.http, response).await {
                        error!("Failed to respond to collect_history - {:?}", why);
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
            new_state.enable().await;
            let mut reader = tokio::io::BufReader::new(stdin).lines();

            while let Ok(Some(line)) = reader.next_line().await {
                if line == "exit" {
                    break;
                }

                let message = InputMessage {
                    content: line,
                    name: "root".to_string(),
                    message_id: "Null".to_string(),
                    reply_to: None,
                    user_id: "Null".to_string(),
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
            CreateCommand::new("deep_search")
                .description("search deeply in internet")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "content", "query to search")
                        .required(true)
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "trials_num", "number of trials")
                        .required(false)
                        .max_int_value(20)
                        .min_int_value(1)
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

    let web_deploy = Arc::new(WebDeploy::new().await);
    web_deploy.start_server("0.0.0.0:80".to_string());
    base_client.def_tool(Arc::new(WebScraper::new()));
    base_client.def_tool(Arc::new(MemoryTool::new()));
    base_client.def_tool(Arc::new(GetTime::new()));
    base_client.def_tool(web_deploy);
    base_client.set_model_config(&conf);
    let base_client = Arc::new(base_client);

    let channels = DashMap::new();


    // Bot のインテント設定（MESSAGE_CONTENT を含む）
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            base_client,
            channels: channels,
        })
        .await
        .expect("Error creating client");

    if let Err(e) = client.start().await {
        error!("Client error: {:?}", e);
    }
}