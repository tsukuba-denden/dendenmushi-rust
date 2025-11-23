use std::time::Instant;

use log::{debug, error, info};
use openai_dive::v1::resources::response::response::Role;
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

#[poise::command(slash_command, prefix_command)]
pub async fn ask(
    ctx: Context<'_>,
    #[description = "モデルに投げるメッセージ"] #[rest] prompt: String,
) -> Result<(), Error> {
    let start = Instant::now();

    if !ctx.data().chat_contexts.is_enabled(ctx.channel_id()) {
        ctx.say("info: Chat context is disabled in this channel. Use !enable to enable it.")
            .await?;
        return Ok(());
    }

    let msg = ctx.say("-# Thinking...").await?;

    let ob_ctx = ctx.data();
    let channel_id = ctx.channel_id();

    // チャンネルごとの LMContext を取得 or 新規作成
    let mut context = ob_ctx.chat_contexts.get_or_create(channel_id);
    let model = ob_ctx.user_contexts.get_or_create(ctx.author().id).main_model.clone();
    let tools = ob_ctx.tools.clone();

    // ユーザーの発話をコンテキストに追加
    context.add_text(prompt.clone(), Role::User);

    let (state_tx, mut state_rx) = tokio::sync::mpsc::channel::<String>(100);
    let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel::<String>(100);
    
    let mut result = None;

    tokio::select! {
        biased;

        r = ob_ctx.lm_client.generate_response(&context, Some(2000), Some(tools), Some(state_tx), Some(delta_tx), Some(model.to_parameter())) => {
            if let Err(e) = &r {
                log_err("Error generating response", e.as_ref());
            }
            result = Some(r?); // ?でエラー処理も可能
            debug!("Response generation completed");
            
        }
        _ = async {
            while let Some(state) = state_rx.recv().await {
                msg.edit(ctx, poise::CreateReply::default().content(format!("-# {}", state))).await.ok();
            }
        } => {}
        _ = async {
            while let Some(delta) = delta_rx.recv().await {
                info!("Delta received: {}", delta);
            }
        } => {}
    }

    let result = result.unwrap();

    // 推論をコンテキストにマージ
    ob_ctx.chat_contexts.marge(channel_id, &result);
    let elapsed = start.elapsed().as_millis();

    // メッセージを編集して最終結果を出す
    let text = &result.get_result();

    debug!("Final response: {}ms \"{}\"", elapsed, text);

    let _msg = ctx.say(text).await?;

    Ok(())
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
