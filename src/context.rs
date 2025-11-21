use std::sync::Arc;

use kurosabi::context::ContextMiddleware;
use log::info;
use openai_dive::v1::api::Client as OpenAIClient;
use wk_371tti_net_crawler::Client as ScraperClient;
use serenity::{Client as DiscordClient, all::GatewayIntents};

use crate::{commands::ping, config::Config};

#[derive(Clone)]
pub struct ObserverContext {
    pub open_ai_client: Arc<OpenAIClient>,
    pub scraper: Arc<ScraperClient>,
    pub config: Arc<Config>,
}

impl ObserverContext {
    pub async fn new() -> ObserverContext {
        let config = Config {
            discord_token: std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set"),
            openai_api_key: std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set"),
        };

        ObserverContext {
            open_ai_client: Arc::new(OpenAIClient::new(config.openai_api_key.clone())),
            scraper: Arc::new(ScraperClient::new().await.unwrap()),
            config: Arc::new(config),
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
                ],
                // prefix の設定（!ping とか）
                prefix_options: poise::PrefixFrameworkOptions {
                    prefix: Some("!".into()),
                    ..Default::default()
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

            
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::GUILDS
            | GatewayIntents::DIRECT_MESSAGES;

        let discord_client = DiscordClient::builder(c.config.discord_token.clone(), intents)
            .framework(framework);

        tokio::spawn(async move {
            discord_client.await.expect("Error creating client").start().await.expect("Error starting client");
        });

    }
}