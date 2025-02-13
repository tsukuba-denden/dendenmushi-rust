use std::sync::Arc;
use agent::{ChannelState, InputMessage};
use dashmap::DashMap;
mod agent;

use call_agent::chat::{
    client::{ModelConfig, OpenAIClient},
    prompt::{Message, MessageContext},
};
use observer::{prefix, tools::{self}};
use tools::{get_time::GetTime, memory::MemoryTool, web_scraper::WebScraper};

use serenity::{all::{CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse}, async_trait};
use serenity::model::gateway::Ready;
use serenity::model::prelude::*;
use serenity::prelude::*;

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
        };

        // このメッセージがBotにメンションされているかどうかを確認
        let bot_id = ctx.cache.current_user().id;
        let is_mentioned = msg.mentions.iter().any(|user| user.id == bot_id);

        // Botにメンションされている場合はAIに質問し、そうでない場合は会話履歴に追加
        if is_mentioned {
            // タイピング表示
            if let Err(e) = msg.channel_id.broadcast_typing(&ctx.http).await {
                println!("Error setting typing indicator: {:?}", e);
            }

            let answer_text = state.ask(message).await;

            if let Err(why) = msg.channel_id.say(&ctx.http, &answer_text).await {
                println!("Err: {:?}", why);
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
                        println!("Failed to respond to ping: {:?}", why);
                    }
                }

                "ask" => {
                    // 考え中
                    let defer_response = CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new()
                    );
                    if let Err(why) = command.create_response(&ctx.http, defer_response).await {
                        println!("Failed to send Defer response: {:?}", why);
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
                    };

                    let answer_text = state.ask(message).await;
                    
                    let response = EditInteractionResponse::new()
                        .content(&answer_text);

                    if let Err(why) = command.edit_response(&ctx.http, response).await {
                        println!("Failed to respond to ask: {:?}", why);
                    }
                }

                "deep_search" => {
                    // 考え中
                    let defer_response = CreateInteractionResponse::Defer(
                        CreateInteractionResponseMessage::new()
                    );
                    if let Err(why) = command.create_response(&ctx.http, defer_response).await {
                        println!("Failed to send Defer response: {:?}", why);
                        return;
                    }
                    let question = command.data.options[0].value.as_str().unwrap();
                    let try_count = command.data.options[1].value.as_i64().unwrap_or(10) as usize;
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
                    };

                    let answer_text = state.deep_search(message, try_count).await;

                    let response = EditInteractionResponse::new()
                        .content(&answer_text);

                    if let Err(why) = command.edit_response(&ctx.http, response).await {
                        println!("Failed to respond to ask: {:?}", why);
                    }
                }


                _ => println!("Unknown command: {}", command.data.name),
            }
        }
    }

    /// Bot が起動したときの処理
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        // グローバルコマンドを登録
        Command::set_global_commands(&ctx.http, vec![
            CreateCommand::new("ping")
                .description("Pong! 🏓")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "返す内", "Pong! 🏓")
                        .required(true)
                ),
            CreateCommand::new("ask")
                .description("observerに話しかける")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "内容", "Observerに質問する内容")
                        .required(true)
                ),
            CreateCommand::new("deep_search")
                .description("observerに深いスクレイピングをさせる")
                .add_option(
                    CreateCommandOption::new(CommandOptionType::String, "内容", "Observerに質問する内容")
                        .required(true)
                )
                .add_option(
                    CreateCommandOption::new(CommandOptionType::Integer, "試行回数", "試行回数")
                        .required(false)
                        .max_int_value(20)
                        .min_int_value(1)
                )
            ])
        .await
        .expect("Failed to create global command");
    }
}

#[tokio::main]
async fn main() {
    // Discord Bot のトークンを取得
    let token = prefix::settings::DISCORD_TOKEN;

    // モデル設定
    let conf = ModelConfig {
        model: "gpt-4o-mini".to_string(),
        model_name: None,
        parallel_tool_calls: None,
        temperature: Some(0.5),
        max_completion_tokens: Some(4000),
        reasoning_effort: None,
        presence_penalty: Some(0.0),
        strict: Some(false),
        top_p: Some(1.0),
    };

    // 基本となる OpenAIClient を生成し、ツールを定義
    let mut base_client = OpenAIClient::new(
        prefix::settings::model::MAIN_MODEL_ENDPOINT,
        Some(prefix::settings::model::MAIN_MODEL_API_KEY),
    );
    base_client.def_tool(Arc::new(GetTime::new()));
    base_client.def_tool(Arc::new(WebScraper::new()));
    base_client.def_tool(Arc::new(MemoryTool::new()));
    base_client.set_model_config(&conf);
    let base_client = Arc::new(base_client);

    let mut c = base_client.create_prompt();
    c.add(vec![Message::User {
        content: vec![MessageContext::Text("こんにちは".to_string())],
        name: Some("Observer".to_string()),
    }])
    .await;

    let rs = c.generate(None).await;

    println!("{:?}", rs);

    let r = c.last().await.unwrap();

    print!("{:?}", r);


    // Bot のインテント設定（MESSAGE_CONTENT を含む）
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            base_client,
            channels: DashMap::new(),
        })
        .await
        .expect("Error creating client");

    if let Err(e) = client.start().await {
        println!("Client error: {:?}", e);
    }
}