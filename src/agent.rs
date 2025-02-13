use std::{result, sync::Arc};

use call_agent::chat::{client::{OpenAIClient, OpenAIClientState}, prompt::{Message, MessageContext}};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct InputMessage {
    pub content: String,
    pub name: String,
    pub message_id: String,
    pub reply_to: Option<String>,
}
// 各チャンネルの会話履歴（state）を保持する構造体
pub struct ChannelState {
    // 並列処理のため、prompt_stream を Mutex で保護する
    prompt_stream: Mutex<OpenAIClientState>,
}

impl ChannelState {
    pub async fn new(client: &Arc<OpenAIClient>) -> Self {
        // 新しい PromptStream を生成する
        let mut prompt_stream = client.create_prompt();
        prompt_stream.set_entry_limit(2000).await;
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        Self {
            prompt_stream: Mutex::new(prompt_stream),
        }
    }

    pub async fn ask(&self, message: InputMessage) -> String {
        let mut prompt_stream = {
            let prompt_stream = self.prompt_stream.lock().await;
            (*prompt_stream).clone()
        };

        let meta = format!("id:{}, replay_to_id:{}", message.message_id, message.reply_to.unwrap_or("none".to_string()));
        let prompt = vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content)
                ],
            name: Some(message.name),
        }];
        prompt_stream.add(prompt).await;

        for _ in 0..5 {
            let res = match prompt_stream.generate_can_use_tool(None).await {
                Ok(res) => res,
                Err(_) => {
                    return "AIからの応答がありませんでした".to_string();
                }
            };
            if res.has_tool_calls {
                continue;
            } else if res.has_content {
                return res.content.unwrap().replace("\\n", "\n");
            } else {
                return "AIからの応答がありませんでした".to_string();
            }
        }
        let res = match prompt_stream.generate(None).await {
            Ok(res) => res,
            Err(_) => {
                return "AIからの応答がありませんでした".to_string();
            }
        };
        if res.has_content {
            return res.content.unwrap().replace("\\n", "\n");
        } else {
            return "AIからの応答がありませんでした".to_string();
        }
    }

    pub async fn deep_search(&self, message: InputMessage, try_count: usize) -> String {
        let mut prompt_stream = {
            let ps = self.prompt_stream.lock().await;
            ps.clone()
        };

        let meta = format!(
            "id:{}, replay_to_id:{}",
            message.message_id,
            message.reply_to.unwrap_or_else(|| "none".to_string())
        );
        let user_prompt = vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content),
            ],
            name: Some(message.name),
        }];
        let system_prompt = vec![Message::Developer {
            content: "p, h1, h2, h3, h4, h5, a, video, img タグを対象に処理します。\n\
                      1. 必要に応じて'https://www.bing.com/search?q={}+{}...'で目的の情報がありそうなページを探します\n\
                      2. ページをスクレイピングして情報を取得します。必要に応じてページにあるリンクのページもスクレイピングして情報を集めます\n\
                      3. もしリンクがなく、解析対象のページがなくなった場合は、スクレイピング方法を変更するか、別のページを検討してください。"
                .to_string(),
            name: Some("Observer".to_string()),
        }];

        prompt_stream.add(user_prompt).await;
        prompt_stream.add(system_prompt).await;

        for _ in 0..try_count {
            let res = match  prompt_stream.generate_with_tool(None, "web_scraper").await {
                Ok(res) => res,
                Err(_) => {
                    return "AIからの応答がありませんでした".to_string();
                }
            };
        }

        prompt_stream
            .add(vec![Message::Developer {
                content: "内容をまとめてください".to_string(),
                name: Some("Observer".to_string()),
            }])
            .await;
        let res = match prompt_stream.generate(None).await {
            Ok(res) => res,
            Err(_) => {
                return "AIからの応答がありませんでした".to_string();
            }
        };
        if res.has_content {
            return res.content.unwrap().replace("\\n", "\n");
        } else {
            return "AIからの応答がありませんでした".to_string();
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