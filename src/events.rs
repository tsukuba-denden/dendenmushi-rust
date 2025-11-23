use std::{error::Error, sync::atomic::AtomicU64, time::{Duration, Instant}};

use log::{debug, info};
use openai_dive::v1::resources::response::{request::{ContentInput, ContentItem, ImageDetailLevel, InputMessage}, response::Role};
use serenity::all::{CreateMessage, EditMessage, FullEvent, Message};
use tokio::{sync::mpsc, time::sleep};


use crate::{commands::log_err, context::ObserverContext, lmclient::LMContext};



pub async fn event_handler(
    ctx: &serenity::client::Context,
    event: &FullEvent,
    framework: poise::FrameworkContext<'_, ObserverContext, Box<dyn Error + Send + Sync>>,
    data: &ObserverContext,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let FullEvent::Message { new_message } = event {
        handle_message(ctx, new_message, framework, data).await?;
    }

    Ok(())
}


async fn handle_message(
    ctx: &serenity::client::Context,
    msg: &Message,
    _framework: poise::FrameworkContext<'_, ObserverContext, Box<dyn Error + Send + Sync>>,
    ob_context: &ObserverContext,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let start = Instant::now();
    // 自分やBOTのメッセージは無視
    if msg.author.bot {
        return Ok(());
    }
    let bot_id = ctx.cache.current_user().id;

    let is_mentioned = msg.mentions_user_id(bot_id);

    let channel_id = msg.channel_id;
    if !ob_context.chat_contexts.is_enabled(channel_id) {
        return Ok(());
    }
    let content = msg.content.clone();

    // 添付画像のURLを取る
    let image_urls: Vec<String> = msg
        .attachments
        .iter()
        .filter(|att| {
            // content_type が "image/..." なら画像とみなす
            if let Some(ct) = &att.content_type {
                ct.starts_with("image/")
            } else {
                // 拡張子で雑に判定する fallback
                att.filename.ends_with(".png")
                    || att.filename.ends_with(".jpg")
                    || att.filename.ends_with(".jpeg")
                    || att.filename.ends_with(".webp")
            }
        })
        .map(|att| att.url.clone())
        .collect();

    let mut lm_context = LMContext::new();

    if image_urls.is_empty() && content.is_empty() {
        // 画像もテキストも無いなら無視
        return Ok(());
    } else if image_urls.is_empty() {
        debug!("Adding text message to context in channel {}, content: {}", channel_id, content);
        lm_context.add_text(content.clone(), Role::User);
    } else {
        debug!("Adding image message to context in channel {}, content: {}", channel_id, content);
        // とりあえず1枚目だけ渡す例。複数渡したければ List に追加
        lm_context.add_message(InputMessage {
            role: Role::User,
            content: ContentInput::List(
                {
                    let mut items = Vec::new();
                    items.push(ContentItem::Text {
                        text: content.clone(),
                    });
                    items.push(ContentItem::Image {
                        detail: ImageDetailLevel::Low,
                        file_id: None,
                        image_url: Some(image_urls[0].clone()),
                    });
                    items
                }
            )
        });
    }

    ob_context.chat_contexts.marge(channel_id, &lm_context);

    if is_mentioned {
        let typing_ctx = ctx.clone();

        let typing_handle = tokio::spawn(async move {
            loop {
                let _ = channel_id.broadcast_typing(&typing_ctx.http).await;
                sleep(Duration::from_secs(5)).await; // だいたい5秒おきでOK
            }
        });
        let context = ob_context.chat_contexts.get_or_create(channel_id);
        let model = ob_context.user_contexts.get_or_create(msg.author.id).main_model.clone();
        let tools = ob_context.tools.clone();

        let mut thinking_msg = msg
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::new().content("-# Thinking..."),
            )
            .await?;

        // streaming 用チャネル
        let (state_tx, mut state_rx) = mpsc::channel::<String>(100);
        let (delta_tx, mut delta_rx) = mpsc::channel::<String>(100);

        let mut result = None;

            
        tokio::select! {
            biased;

            r = ob_context.lm_client.generate_response(&context, Some(2000), Some(tools), Some(state_tx), Some(delta_tx), Some(model.to_parameter())) => {
                if let Err(e) = &r {
                    log_err("Error generating response", e.as_ref());
                    thinking_msg
                        .edit(&ctx.http, EditMessage::new().content("-# Error during reasoning"))
                        .await
                        .ok();
                    typing_handle.abort();
                }
                result = Some(r?); // ?でエラー処理も可能
                debug!("Response generation completed");
                
            }
            _ = async {
                let mut last_edit = Instant::now() + Duration::from_secs(1);      // 前回 edit した時間
                let mut swap = String::new();          // 状態保存用バッファ

                while let Some(state) = state_rx.recv().await {
                    swap = state;

                    // 1秒未満ならまだ edit しない
                    if last_edit.elapsed() < Duration::from_secs(1) {
                        continue;
                    }

                    // 1秒経過 → 最新 state だけ使って edit
                    thinking_msg
                        .edit(&ctx.http, EditMessage::new().content(format!("-# {}", swap)))
                        .await
                        .ok();
                    last_edit = Instant::now(); // 時刻更新
                }
            } => {}
            _ = async {
                while let Some(delta) = delta_rx.recv().await {
                    info!("Delta received: {}", delta);
                }
            } => {}
        }
    
        let result = result.unwrap();

        // 返ってきた結果をコンテキストにマージ
        ob_context.chat_contexts.marge(channel_id, &result);

        let elapsed = start.elapsed().as_millis();
        let text = result.get_result();

        debug!("Final response: {}ms \"{}\"", elapsed, text);

        // タイピング通知停止
        typing_handle.abort();

        // 「Thinking...」を書き換えて最終回答を表示
        thinking_msg
            .edit(&ctx.http, EditMessage::new().content("-# Reasoning done"))
            .await?;
        msg.channel_id
            .send_message(&ctx.http, CreateMessage::new().content(text.clone()))
            .await?;
    }


    Ok(())
}
