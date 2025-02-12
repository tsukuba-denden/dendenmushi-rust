use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::Mutex;

use call_agent::chat::{
    client::{ModelConfig, OpenAIClient, OpenAIClientState},
    prompt::{Message, MessageContext},
};
use observer::{prefix, tools::{self, web_scraper}};
use tools::{get_time::GetTime, memory::MemoryTool, web_scraper::WebScraper};

use serenity::{all::{CreateCommand, CreateCommandOption, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse}, async_trait};
use serenity::model::gateway::Ready;
use serenity::model::prelude::*;
use serenity::prelude::*;

pub struct InputMessage {
    pub content: String,
    pub name: String,
    pub message_id: String,
    pub reply_to: Option<String>,
}
// 各チャンネルの会話履歴（state）を保持する構造体
pub struct ChannelState {
    // 並列処理のため、prompt_stream を Mutex で保護する
    prompt_stream: Mutex<OpenAIClientState<'static>>,
}

impl ChannelState {
    fn new(client: &Arc<OpenAIClient>) -> Self {
        // 新しい PromptStream を生成する
        let prompt_stream = client.create_prompt();
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        let prompt_stream: OpenAIClientState<'static> = unsafe { std::mem::transmute(prompt_stream) };
        Self {
            prompt_stream: Mutex::new(prompt_stream),
        }
    }

    pub async fn ask(&self, message: InputMessage) -> String {
        let mut prompt_stream = {
            let prompt_stream = self.prompt_stream.lock().await;
            (*prompt_stream).clone()
        };

        let content = format!("id:{};\n{}", message.message_id, message.content);

        let prompt = vec![Message::User {
            content: vec![MessageContext::Text(content)],
            name: Some(message.name),
        }];
        prompt_stream.add(prompt).await;

        for _ in 0..5 {
            let _ = prompt_stream.generate_can_use_tool(None).await;
            let res = match prompt_stream.last().await {
                Some(r) => r,
                None => return "AIからの応答がありませんでした".to_string(),
            };

            println!("{:?}", res);

            match res {
                Message::Tool { .. } => continue,
                Message::Assistant { ref content, .. } => {
                    if let Some(MessageContext::Text(text)) = content.first() {
                        return text.replace("\\n", "\n");
                    } else {
                        return format!("{:?}", res);
                    }
                }
                _ => return "AIからの応答がありませんでした".to_string(),
            }
        }
        let _ = prompt_stream.generate(None).await;
        let res = prompt_stream.last().await.unwrap();
        println!("{:?}", res);
        match res {
            Message::Assistant { ref content, .. } => {
                if let Some(MessageContext::Text(text)) = content.first() {
                    return text.replace("\\n", "\n");
                } else {
                    return format!("{:?}", res);
                }
            }
            _ => return "AIからの応答がありませんでした".to_string(),
        }
    }

    pub async fn deep_search(&self, message: InputMessage, try_count: usize) -> String {
        let mut prompt_stream = {
            let prompt_stream = self.prompt_stream.lock().await;
            (*prompt_stream).clone()
        };

        let content = format!("id:{};\n{}", message.message_id, message.content);

        let prompt = vec![Message::User {
            content: vec![MessageContext::Text(content)],
            name: Some(message.name),
        }];

        let systemprompt = vec![Message::Developer {
            content: "p, h1, h2, h3, h4, h5, a, video, imgタグを指定してリンクをたどったりして内容を完全に把握するように まずは初めのページあるリンクなどをたどっていくように そのページの要素をすべて確認したら そのページのリンクのページを続けてみていくように なにも見つからなかったらスクレイピング方法を変えるか、ほかのページを見に行ってください".to_string(),
            name: Some("Observer".to_string()),
        }];
        prompt_stream.add(prompt).await;
        prompt_stream.add(systemprompt).await;

        for _ in 0..try_count {
            let _ = prompt_stream.generate_with_tool(None, "web_scraper").await;
            let res = match prompt_stream.last().await {
                Some(r) => r,
                None => return "AIからの応答がありませんでした".to_string(),
            };

            println!("{:?}", res);

            match res {
                Message::Tool { .. } => continue,
                Message::Assistant { ref content, .. } => {
                    if let Some(MessageContext::Text(text)) = content.first() {
                        return text.replace("\\n", "\n");
                    } else {
                        return format!("{:?}", res);
                    }
                }
                _ => return "AIからの応答がありませんでした".to_string(),
            }
        }
        prompt_stream.add(
            vec![Message::Developer {
                content: "検索でみつけた内容をまとめてください".to_string(),
                name: Some("Observer".to_string()),
            }]
        ).await;
        let _ = prompt_stream.generate(None).await;
        let res = prompt_stream.last().await.unwrap();
        println!("{:?}", res);
        match res {
            Message::Assistant { ref content, .. } => {
                if let Some(MessageContext::Text(text)) = content.first() {
                    return text.replace("\\n", "\n");
                } else {
                    return format!("{:?}", res);
                }
            }
            _ => return "AIからの応答がありませんでした".to_string(),
        }
    }

    pub async fn add_message(&self, message: InputMessage) {
        let mut prompt_stream = self.prompt_stream.lock().await;

        let content = format!("id:{};\n{}", message.message_id, message.content);

        let prompt = vec![Message::User {
            content: vec![MessageContext::Text(content)],
            name: Some(message.name),
        }];
        prompt_stream.add(prompt).await;
    }
}

struct Handler {
    // Handlerに1つのOpenAIClientを保持
    base_client: Arc<OpenAIClient>,
    // 各チャンネルごとの状態（会話履歴）を保持（DashMapは並列処理可能）
    channels: DashMap<ChannelId, Arc<ChannelState>>,
}

#[async_trait]
impl EventHandler for Handler {
    /// メッセージが送信されたときの処理
    async fn message(&self, _ctx: Context, msg: serenity::all::Message) {
        let state = self
            .channels
            .entry(msg.channel_id)
            .or_insert_with(|| {
                Arc::new(ChannelState::new(&self.base_client))
            })
            .clone();

        let message = InputMessage {
            content: msg.content,
            name: msg.author.name.clone(),
            message_id: msg.id.to_string(),
            reply_to: None,
        };

        state.add_message(message).await;
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
                    let state = self
                        .channels
                        .entry(command.channel_id)
                        .or_insert_with(|| {
                            Arc::new(ChannelState::new(&self.base_client))
                        })
                        .clone();

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
                    let state = self
                        .channels
                        .entry(command.channel_id)
                        .or_insert_with(|| {
                            Arc::new(ChannelState::new(&self.base_client))
                        })
                        .clone();

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