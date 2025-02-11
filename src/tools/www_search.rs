
use call_agent::chat::function::Tool;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
}

#[derive(Clone)]
pub struct WebSearch {
    client: Client,
}

impl WebSearch {
    /// `WebSearch` のインスタンスを生成
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 Chrome/91.0.4472.124 Safari/537.36 call-agent/0.1 Observer/0.1")
            .build()
            .expect("Failed to build reqwest client");
        WebSearch { client }
    }

    /// DuckDuckGo で検索を実行する（非同期関数）
    pub async fn search(
        &self,
        query: &str,
        max_results: usize,
        location: Option<&str>,
        safe_search: Option<&str>,
        search_type: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn Error + Send + Sync>> {
        // デフォルトパラメータの設定
        let region = location.unwrap_or("us-en");
        let safe = safe_search.unwrap_or("moderate");
        let _search_type = search_type.unwrap_or("web");

        // クエリを URL エンコード
        let encoded_query = urlencoding::encode(query);
        let url_str = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_redirect=1&no_html=1&region={}&safe={}",
            encoded_query, region, safe
        );
        let url = Url::parse(&url_str)?;

        // HTTP リクエストを送信
        let response = self.client.get(url).send().await?.error_for_status()?;
        let json_value: serde_json::Value = response.json().await?;

        // (デバッグ用) レスポンス全体を確認する場合は下記のコメントアウトを外す
        println!("Response JSON: {:#?}", json_value);

        let mut results = Vec::new();

        if let Some(related_topics) = json_value.get("RelatedTopics").and_then(|v| v.as_array()) {
            for topic in related_topics.iter() {
                // ネストされた Topics があればすべて走査
                if let Some(nested_topics) = topic.get("Topics").and_then(|v| v.as_array()) {
                    for nested in nested_topics {
                        if let (Some(text), Some(link)) = (
                            nested.get("Text").and_then(|v| v.as_str()),
                            nested.get("FirstURL").and_then(|v| v.as_str()),
                        ) {
                            results.push(SearchResult {
                                title: text.to_string(),
                                link: link.to_string(),
                            });
                        }
                    }
                }
                // ネストされていない単一トピックの場合
                else if let (Some(text), Some(link)) = (
                    topic.get("Text").and_then(|v| v.as_str()),
                    topic.get("FirstURL").and_then(|v| v.as_str()),
                ) {
                    results.push(SearchResult {
                        title: text.to_string(),
                        link: link.to_string(),
                    });
                }
            }
        }
        // ここではすべての結果を取得してから、max_results 件に絞っています
        Ok(results.into_iter().take(max_results).collect())
    }
}

impl Tool for WebSearch {
    fn def_name(&self) -> &str {
        "web_search"
    }

    fn def_description(&self) -> &str {
        "Performs a web search using DuckDuckGo with optional location and filters"
    }

    fn def_parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query string"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Number of top search results to return",
                    "minimum": 1,
                    "maximum": 10,
                    "default": 5
                },
                "location": {
                    "type": "string",
                    "description": "The region code for search results (e.g., 'us-en', 'jp-ja')",
                    "default": "us-en"
                },
                "safe_search": {
                    "type": "string",
                    "enum": ["on", "moderate", "off"],
                    "description": "Safe search filter level",
                    "default": "moderate"
                },
                "search_type": {
                    "type": "string",
                    "enum": ["web", "images", "news"],
                    "description": "Type of search (web, images, news)",
                    "default": "web"
                }
            },
            "required": ["query"]
        })
    }

    fn run(&self, args: serde_json::Value) -> Result<String, String> {
        let query = args.get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'query' parameter".to_string())?
            .to_string();
        let max_results = args.get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;
        let location = args.get("location").and_then(|v| v.as_str()).map(|s| s.to_string());
        let safe_search = args.get("safe_search").and_then(|v| v.as_str()).map(|s| s.to_string());
        let search_type = args.get("search_type").and_then(|v| v.as_str()).map(|s| s.to_string());

        let web_search = self.clone();

        // 新しいスレッド上で非同期処理を実行する
        let result = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(web_search.search(
                &query,
                max_results,
                location.as_deref(),
                safe_search.as_deref(),
                search_type.as_deref(),
            ))
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())?
        .map_err(|e| format!("Search error: {}", e))?;

        let response = if result.is_empty() {
            "No results found.".to_string()
        } else {
            result.into_iter()
                .map(|r| format!("- {} - {}", r.title, r.link))
                .collect::<Vec<String>>()
                .join("\n")
        };

        Ok(response)
    }
}
