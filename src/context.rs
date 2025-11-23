use std::{collections::HashMap, sync::Arc};

use kurosabi::context::ContextMiddleware;
use log::info;
use openai_dive::v1::api::Client as OpenAIClient;
use wk_371tti_net_crawler::Client as ScraperClient;
use serenity::{Client as DiscordClient, all::GatewayIntents};

use crate::{channel::ChatContexts, commands::{ask, clear, disable, enable, model, ping}, config::Config, events::event_handler, lmclient::{LMClient, LMTool}, tools, user::UserContexts};

#[derive(Clone)]
pub struct ObserverContext {
    pub lm_client: Arc<LMClient>,
    pub scraper: Arc<ScraperClient>,
    pub config: Arc<Config>,
    pub chat_contexts: Arc<ChatContexts>,
    pub user_contexts: Arc<UserContexts>,
    pub tools: Arc<HashMap<String, Box<dyn LMTool>>>,
}

impl ObserverContext {
    pub async fn new() -> ObserverContext {
        let config = Config::new();

        let lm_client = LMClient::new(OpenAIClient::new(config.openai_api_key.clone()));
        let tools: HashMap<String, Box<dyn LMTool>> = vec![
            Box::new(tools::get_time::GetTime::new()) as Box<dyn LMTool>,
        ]
        .into_iter()
        .map(|tool| (tool.name(), tool))
        .collect();

        ObserverContext {
            lm_client: Arc::new(lm_client),
            scraper: Arc::new(ScraperClient::new().await.unwrap()),
            config: Arc::new(config),
            chat_contexts: Arc::new(ChatContexts::new()),
            user_contexts: Arc::new(UserContexts::new()),
            tools: Arc::new(tools),
        }
    }

    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.scraper.engine.shutdown().await;
        Ok(())
    }
}
#[async_trait::async_trait]
impl ContextMiddleware<ObserverContext> for ObserverContext {
    async fn init(c: ObserverContext) {
        info!("Starting Discord bot...");
        let ob_ctx = c.clone();
        let framework = poise::Framework::builder()
            .options(poise::FrameworkOptions {
                commands: vec![
                    ping(),  // ここにコマンドを追加
                    ask(),
                    enable(),
                    clear(),
                    disable(),
                    model()
                ],
                // prefix の設定（!ping とか）
                prefix_options: poise::PrefixFrameworkOptions {
                    prefix: Some("!".into()),
                    ..Default::default()
                },
                event_handler: |ctx, event, framework, data| {
                    Box::pin(event_handler(ctx, event, framework, data))
                },
                ..Default::default()
            })
            // 起動時に一度だけ呼ばれるセットアップ処理
            .setup(|ctx, _ready, framework| {
                Box::pin(async move {
                    // Slash コマンドをグローバル登録
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    println!("Bot is ready!");
                    Ok(ob_ctx)
                })
            })
            .build();

            
        let intents =
            GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::GUILD_MESSAGE_REACTIONS // (必要なら)
            | GatewayIntents::MESSAGE_CONTENT;


        let discord_client = DiscordClient::builder(c.config.discord_token.clone(), intents)
            .framework(framework);

        tokio::spawn(async move {
            discord_client.await.expect("Error creating client").start().await.expect("Error starting client");
        });

    }
}

