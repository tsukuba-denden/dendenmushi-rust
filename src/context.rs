use std::{collections::HashMap, sync::{Arc, RwLock}};

use kurosabi::context::ContextMiddleware;
use log::info;
use openai_dive::v1::api::Client as OpenAIClient;
use wk_371tti_net_crawler::Client as ScraperClient;
use serenity::{Client as DiscordClient, all::GatewayIntents};

use crate::{channel::ChatContexts, commands::{clear, disable, enable, model, ping}, config::Config, events::event_handler, lmclient::{LMClient, LMTool}, tools, user::UserContexts};

#[derive(Clone)]
pub struct ObserverContext {
    pub lm_client: Arc<LMClient>,
    pub scraper: Arc<ScraperClient>,
    pub config: Arc<Config>,
    pub chat_contexts: Arc<ChatContexts>,
    pub user_contexts: Arc<UserContexts>,
    pub tools: Arc<HashMap<String, Box<dyn LMTool>>>,
    pub discord_client: Arc<DiscordContextWrapper>,
}

/// DiscordContext を全体共有するための頭の悪いラッパー
pub struct DiscordContextWrapper {
    pub inner: RwLock<Option<Arc<DisabledContextWrapperInner>>>,
}

impl DiscordContextWrapper {
    pub fn open(&self) -> Arc<DisabledContextWrapperInner> {
        self.inner.read().expect("RWlock").clone().expect("inisializing").clone()
    }
    pub fn lazy() -> DiscordContextWrapper {
        DiscordContextWrapper {
            inner: RwLock::new(None),
        }
    }
    pub fn set(&self, ctx: Arc<DisabledContextWrapperInner>) {
        let mut w = self.inner.write().expect("RWlock");
        *w = Some(ctx);
    }
}

pub struct DisabledContextWrapperInner {
    pub http: Arc<serenity::http::Http>,
    pub cache: Arc<serenity::cache::Cache>,
}

impl ObserverContext {
    pub async fn new() -> ObserverContext {
        let config = Config::new();

        let lm_client = LMClient::new(OpenAIClient::new(config.openai_api_key.clone()));
        let tools: HashMap<String, Box<dyn LMTool>> = vec![
            Box::new(tools::get_time::GetTime::new()) as Box<dyn LMTool>,
            Box::new(tools::browser::Browser::new()) as Box<dyn LMTool>,
            Box::new(tools::discord::DiscordToolReaction::new()) as Box<dyn LMTool>,
            Box::new(tools::discord::DiscordToolThread::new()) as Box<dyn LMTool>,
            Box::new(tools::discord::DiscordToolFetchMessage::new()) as Box<dyn LMTool>,
            Box::new(tools::discord::DiscordToolSendMessage::new()) as Box<dyn LMTool>,
            Box::new(tools::discord::DiscordToolSearchMessages::new()) as Box<dyn LMTool>,
            Box::new(tools::discord::DiscordToolEditMessage::new()) as Box<dyn LMTool>,
        ]
        .into_iter()
        .map(|tool| (tool.name(), tool))
        .collect();

        ObserverContext {
            lm_client: Arc::new(lm_client),
            scraper: Arc::new(ScraperClient::new("http://192.168.0.81")),
            config: Arc::new(config),
            chat_contexts: Arc::new(ChatContexts::new()),
            user_contexts: Arc::new(UserContexts::new()),
            tools: Arc::new(tools),
            discord_client: Arc::new(DiscordContextWrapper::lazy()),
        }
    }

    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Shutting down ObserverContext...");
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
                    ob_ctx.discord_client.set(Arc::new(DisabledContextWrapperInner {
                        http: ctx.http.clone(),
                        cache: ctx.cache.clone(),
                    }));
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
            let mut c = discord_client.await.expect("Error creating client");
            c.start().await.expect("Error starting client");
            
        });

    }
}

