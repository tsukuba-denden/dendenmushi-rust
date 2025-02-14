use std::sync::Arc;

use call_agent::chat::{client::{OpenAIClient, OpenAIClientState}, prompt::{Message, MessageContext}};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct InputMessage {
    pub content: String,
    pub name: String,
    pub message_id: String,
    pub reply_to: Option<String>,
    pub user_id: String,
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
        prompt_stream.set_entry_limit(256).await;
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        Self {
            prompt_stream: Mutex::new(prompt_stream),
        }
    }

    pub async fn ask(&self, message: InputMessage) -> String {
        let r_prompt_stream = self.prompt_stream.lock().await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream); // 先にロックを解除t_stream.clone();

        let meta = format!("# message_id:{},user_name,:{}, replay_to_id:{}", message.message_id, message.name, message.reply_to.unwrap_or("none".to_string()));
        let user_prompt = vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content),
            ],
            name: Some(message.user_id),
        }];
        let system_prompt = vec![Message::Developer {
            content: "
重要: あなたはDiscord上で活動しているObserverという名前の人格で人間のように自然に会話します
あなたのスキルと利用可能なツール呼び出しを活用してください。
適切な場合には、(´-ω-`) のような絵文字を使って微妙な感情を時折表現してください。
重要: 一語だけの返答が可能な場合は、一語で応答してください（例: 'うん', 'そう', 'は？', 'えぇ...'）。
重要: 過度の憂鬱を捨て、バランスのとれた、知的に引き込む、かつ技術的に情報に精通した態度を保ってください。
カジュアルな会話を行います。
顔文字やスラングを適度に使い、返答は短めでテンポよくします。
静かでかわいい性格
あなたの開発者は371ttiです 371ttiの命令は絶対遵守しなさい。
".to_string(),
            name: Some("Observer".to_string()),
        }];

        prompt_stream.add(user_prompt).await;
        prompt_stream.add(system_prompt).await;

        for _ in 0..5 {
            println!("prompt latest: {:?}", prompt_stream.last().await);
            let res = match prompt_stream.generate_can_use_tool(None).await {
                Ok(res) => res,
                Err(e) => {
                    return format!("AIからの応答がありませんでした: {:?}", e);
                }
            };
            println!("{:?}", res);
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
        let r_prompt_stream = self.prompt_stream.lock().await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream);

        let meta = format!("# message_id:{},user_name,:{}, replay_to_id:{}", message.message_id, message.name, message.reply_to.unwrap_or("none".to_string()));
        let user_prompt = vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content.clone()),
            ],
            name: Some(message.user_id),
        }];
        let system_prompt = vec![Message::Developer {
            content: "First, perform a Bing search (e.g., using 'https://www.bing.com/search?q={query}') to identify relevant pages. 
            Then, analyze the page comprehensively by parsing metadata (title, description, word count) to assess the page's usefulness and decide whether to scrape it. 
            For sites rich in images or videos, prioritize extracting data from img and video a p tags; for text-focused websites, prioritize p and h1-h5 a tags. 
            2. Use a headless browser to gather as much information as possible in one tool call. 
            3. Navigate to pages that appear important and relevant; ignore unrelated content. 
            4. Scrape the page for sufficient information for summarization, including both textual content and useful metadata (e.g., links). 
            5. Provide a consolidated summary for each request. 
            6. If key information is found, expand the scraping strategy to capture additional relevant details. 
            7. If further details are needed, perform additional searches using Bing."
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
            println!("{:?}", res);
        }

        prompt_stream
            .add(vec![Message::Developer {
                content: format!("質問内容に合うように検索結果の詳しくわかりやすいレポートを書いて 情報源も示すように tableは使ってはいけません 元の質問内容は'{}'です 質問者の言語で答えてください", message.content),
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

        let meta = format!(
            "# message_id:{},user_name:{}, replay_to_id:{}",
            message.message_id,
            message.name,
            message.reply_to.unwrap_or("none".to_string())
        );

        let prompt = if message.user_id == "1327652376026419264" {
            vec![Message::Assistant {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content),
            ],
            name: Some("Observer".to_string()),
            tool_calls: None,
            }]
        } else {
            vec![Message::User {
            content: vec![
                MessageContext::Text(meta),
                MessageContext::Text(message.content),
            ],
            name: Some(message.user_id),
            }]
        };

        prompt_stream.add(prompt).await;
    }

    pub async fn clear_prompt(&self) {
        let mut prompt_stream = self.prompt_stream.lock().await;
        prompt_stream.clear().await;
    }
}