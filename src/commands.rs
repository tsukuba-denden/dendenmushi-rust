use std::time::Instant;

use poise::CreateReply;

use crate::context::ObserverContext;

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
