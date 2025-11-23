use std::time::Instant;

use log::{error, info};
use poise::CreateReply;

use crate::{config::Models, context::ObserverContext};

// エラー型（とりあえず Box に投げるスタイルでOK）
type Error = Box<dyn std::error::Error + Send + Sync>;

// 毎回書くのがだるいので type alias
type Context<'a> = poise::Context<'a, ObserverContext, Error>;

#[poise::command(slash_command, prefix_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let start = Instant::now();

    // まずメッセージ送信
    let msg = ctx.say("Pinging...").await?;

    let elapsed = start.elapsed().as_millis();

    // CreateReply を作って渡す
    msg.edit(
        ctx,
        CreateReply::default().content(format!("Pong! `{elapsed}ms`")),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();

    let ob_ctx = ctx.data();

    ob_ctx.chat_contexts.clear(channel_id);

    info!("Cleared chat context for channel {}", channel_id);

    ctx.say("info: Cleared chat context.").await?;

    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn enable(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();

    let ob_ctx = ctx.data();

    if ob_ctx.chat_contexts.is_enabled(channel_id) {
        ctx.say("info: Chat context is already enabled in this channel.").await?;
        return Ok(());
    } else {
        ob_ctx.chat_contexts.set_enabled(channel_id, true);
        ob_ctx.chat_contexts.get_or_create(channel_id);
        ctx.say("info: Chat context enabled in this channel.").await?;
        info!("Enabled chat context for channel {}", channel_id);
        return Ok(());
    }
}

#[poise::command(slash_command, prefix_command)]
pub async fn disable(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();

    let ob_ctx = ctx.data();

    if !ob_ctx.chat_contexts.is_enabled(channel_id) {
        ctx.say("info: Chat context is already disabled in this channel.").await?;
        return Ok(());
    } else {
        ob_ctx.chat_contexts.set_enabled(channel_id, false);
        ctx.say("info: Chat context disabled in this channel.").await?;
        info!("Disabled chat context for channel {}", channel_id);
        return Ok(());
    }
}

#[poise::command(slash_command, prefix_command, subcommands("get", "set", "list"))]
pub async fn model(_: Context<'_>) -> Result<(), Error> {
    Ok(()) // ここはメインでは使わない
}

#[poise::command(slash_command, prefix_command)]
pub async fn get(ctx: Context<'_>) -> Result<(), Error> {
    let ob_ctx = ctx.data();
    let user_id = ctx.author().id;
    let model = ob_ctx.user_contexts.get_or_create(user_id).main_model.clone();
    ctx.say(format!("Current model: `{}`", model)).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let models = Models::list();

    let mut s = String::from("**List of models:**\n");
    for m in models {
        s.push_str(&format!("- `{}`\n", m));
    }

    ctx.say(s).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "Choose a model"]
    #[autocomplete = "autocomplete_model_name"]
    model_name: String,
) -> Result<(), Error> {
    let ob_ctx = ctx.data();
    let user_id = ctx.author().id;
    let model = Models::from(model_name);
    ob_ctx.user_contexts.set_model(user_id, model.clone());

    ctx.say(format!("info: Changed model to `{}`", model)).await?;
    Ok(())
}

async fn autocomplete_model_name(
    _ctx: Context<'_>,
    partial: &str,
) -> Vec<String> {
    let models = Models::list();
    models
        .into_iter()
        .filter(|m| m.to_string().starts_with(partial))
        .map(|m| m.to_string())
        .collect()
}

pub fn log_err(context: &str, err: &(dyn std::error::Error + Send + Sync)) {
    error!("[{context}] {err:#?}");

    let mut src = err.source();
    while let Some(s) = src {
        error!("  caused by: {s:?}");
        src = s.source();
    }
    error!("error trace end");
}
