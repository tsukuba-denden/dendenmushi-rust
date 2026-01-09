use super::web_scraper::Browser as WebBrowser;
use call_agent::chat::{
    client::OpenAIClient,
    function::Tool,
    prompt::{Message, MessageContext},
};
use log::info;
use regex::Regex;
use serde_json::Value;
use tokio::runtime::Runtime;

/// **テキストの長さを計算するツール**
pub struct BrowsingWorker {
    pub model: OpenAIClient,
}

impl BrowsingWorker {
    pub fn new(model: OpenAIClient) -> Self {
        Self { model }
    }
}

impl Tool for BrowsingWorker {
    fn def_name(&self) -> &str {
        "browsing_worker"
    }

    fn def_description(&self) -> &str {
        "Get a summary of the web page. If you want to obtain the original page without summarization, please use your browser. You can also provide instructions in natural language along with the URL. It can generate a summary of the entire page very quickly. Please note that others cannot see your response."
    }

    fn def_parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "URL and some natural language query ex.Gather links to the materials located at https://*.*/*/... . please write with user used language eg."
                },
                "$explain": {
                    "type": "string",
                    "description": "A brief explanation of what you are doing with this tool."
                },
            },
            "required": ["query"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        info!("BrowsingWorker::run called with args: {:?}", args);
        let query = args["query"]
            .as_str()
            .ok_or_else(|| "Missing 'query' parameter".to_string())?
            .to_string();

        // クエリ内にURLが含まれている場合は、実際にそのURLを取得して要約する
        if let Some(url) = extract_first_url(&query) {
            // 安全なURLのみ許可
            if !WebBrowser::is_safe_url(&url) {
                return Err("Are you try hacking me?".to_string());
            }

            let result = std::thread::spawn(move || -> Result<String, String> {
                let rt = Runtime::new().expect("Failed to create runtime");
                let scraper = WebBrowser::new();
                let data = rt
                    .block_on(scraper.scrape_reqwest(&url, "p, h1, h2, h3, a"))
                    .map_err(|e| format!("Scrape error: {}", e))?;
                let summary = WebBrowser::compress_content(data, 0, 2000);
                Ok(format!("{}\n\nURL: {}", summary, url))
            })
            .join()
            .map_err(|_| "Thread panicked".to_string())??;

            return Ok(serde_json::json!({ "Summary": result }).to_string());
        }

        // URLが含まれない場合は、検索で取得して要約
        match search_and_summarize(&query) {
            Ok(summary) => {
                return Ok(serde_json::json!({ "Summary": summary }).to_string());
            }
            Err(_) => {
                // 検索に失敗した場合は LLM フォールバック
            }
        }

        let mut model = self.model.clone().create_prompt();

        let result = std::thread::spawn(move || -> Result<String, String> {
            let rt = Runtime::new().expect("Failed to create runtime");
            let messages = vec![
                Message::System {
                    name: Some("owner".to_string()), 
                    content: "You are an excellent AI assistant who searches for web pages regarding the request content and faithfully summarizes the entire content of that page. Absolutely use the internet to research and compile information.Also, be sure to indicate the source (URL).".to_string() 
                },
                Message::User {
                    name: Some("observer".to_string()),
                    content: vec![
                        MessageContext::Text(query.clone()),
                    ],
                }
            ];

            // モデルに投げる
            let res: String = rt.block_on(async {
                model.add(messages).await;
                let return_value = model.generate(None).await.map_err(|_| "Failed to generate".to_string())?;
                let mut string = return_value.content.ok_or("Failed to result".to_string())?;
                // 注釈（URL引用）が存在する場合のみ安全に抽出
                let captions = return_value
                    .api_result
                    .response
                    .choices
                    .as_ref()
                    .and_then(|choices| choices.first())
                    .and_then(|c| c.message.annotations.as_ref())
                    .and_then(|ann| ann.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                v.as_object()
                                    .and_then(|o| o.get("url_citation"))
                                    .and_then(|u| u.as_object())
                                    .and_then(|u| u.get("url"))
                                    .and_then(|u| u.as_str())
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                if !captions.is_empty() {
                    string = format!("{}\n\nLinks: {}", string, captions);
                }
                Ok(string)
            }).map_err(|e: String| e.to_string())?;

            Ok(res)
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())??;

        // JSONで結果を返す
        Ok(serde_json::json!({ "Summary": result }).to_string())
    }
}

// クエリ文字列から最初のURLを抽出
fn extract_first_url(text: &str) -> Option<String> {
    // シンプルなURL検出（http/httpsで始まる空白区切りのトークン）
    let re = Regex::new(r"https?://[^\s]+").ok()?;
    re.find(text).map(|m| m.as_str().to_string())
}

// URLが含まれない場合の検索処理（Bing）
fn search_and_summarize(query: &str) -> Result<String, String> {
    let query = query.to_string(); // Clone the query string to own it
    let search_url = format!(
        "https://www.bing.com/search?q={}",
        urlencoding::encode(&query)
    );
    if !WebBrowser::is_safe_url(&search_url) {
        return Err("Are you try hacking me?".to_string());
    }

    let result = std::thread::spawn(move || -> Result<String, String> {
        let rt = Runtime::new().expect("Failed to create runtime");
        let scraper = WebBrowser::new();
        let data = rt
            .block_on(scraper.scrape_reqwest(&search_url, "a, h2, h3"))
            .map_err(|e| format!("Scrape error: {}", e))?;

        // 上位の外部リンクを抽出
        let mut links: Vec<(String, String)> = data
            .items
            .into_iter()
            .filter_map(|it| match it.link {
                Some(link) if link.starts_with("http") && !link.contains("bing.com") => {
                    let title = if it.text.trim().is_empty() {
                        link.clone()
                    } else {
                        it.text
                    };
                    Some((title, link))
                }
                _ => None,
            })
            .collect();

        // 重複除去（リンク基準）
        links.sort_by(|a, b| a.1.cmp(&b.1));
        links.dedup_by(|a, b| a.1 == b.1);

        // 先頭のリンクを簡易要約（本文の先頭を圧縮）
        let summary = if let Some((top_title, top_link)) = links.first() {
            let top_data = rt
                .block_on(scraper.scrape_reqwest(top_link, "p, h1, h2, h3, a"))
                .ok();
            let brief = top_data
                .map(|d| WebBrowser::compress_content(d, 0, 800))
                .unwrap_or_else(|| String::from("(内容の抽出に失敗しました)"));
            format!(
                "検索: {}\n上位: {}\n\n抜粋:\n{}\n\nSources:\n{}\n{}",
                query,
                top_title,
                brief,
                top_link,
                links
                    .iter()
                    .skip(1)
                    .take(4)
                    .map(|(_, l)| l.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            // リンクのみ列挙
            let list = links
                .iter()
                .take(5)
                .map(|(t, l)| format!("- {}\n  {}", t, l))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "検索: {}\nリンク:\n{}\n\nSearch URL: {}",
                query, list, search_url
            )
        };

        Ok(summary)
    })
    .join()
    .map_err(|_| "Thread panicked".to_string())??;

    Ok(result)
}
