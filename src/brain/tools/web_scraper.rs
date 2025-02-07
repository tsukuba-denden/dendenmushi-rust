/*!
Webスクレイピングツール (Rust版)

このツールは指定されたURLからHTMLページを取得し、ユーザーが指定する
CSSセレクターに基づいて、該当する要素のテキストやリンク情報を抽出します。

【CSSセレクターとは？】
CSSセレクターは、HTML内の特定の要素を選択するためのルールです。
Webスクレイピングでは、抽出したい情報がどのタグやクラス、IDなどに含まれているかを指定するために利用します。

例えば：
- `p`              → すべての段落タグ `<p>` を選択
- `.headline`      → クラス名が "headline" の要素を選択
- `#main`          → IDが "main" の要素を選択
- `div > p`        → 直下の `<p>` 要素を選択
- `a[href="..."]`  → 指定の href 属性を持つリンクを選択

【使い方の例】
以下のJSONパラメータを渡すと、指定したURLのページから「p, h1, h2, h3, h4, h5, h6, a」タグの内容とリンク情報が抽出され、JSON形式で返されます：
{
    "url": "https://example.com",
    "selector": "p, h1, h2, h3, h4, h5, h6, a"
}
*/

use call_agent::function::Tool;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;
#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapedItem {
    /// 要素内のテキスト（余分な空白は除去済み）
    pub text: String,
    /// リンク要素の場合、href属性の値（それ以外は None）
    pub link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapedData {
    pub items: Vec<ScrapedItem>,
}

#[derive(Clone)]
pub struct WebScraper {
    client: Client,
}

impl WebScraper {
    /// 新しいWebScraperインスタンスを生成する
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; WebScraper/1.0)")
            .build()
            .expect("Failed to build reqwest client");
        WebScraper { client }
    }

    /// 指定されたURLからページを取得し、CSSセレクターで要素を抽出する
    ///
    /// # 引数
    /// * `url` - スクレイピング対象のページURL
    /// * `selector_str` - 抽出したい要素を指定するCSSセレクター
    ///
    /// # 戻り値
    /// 抽出結果として、各要素のテキストとリンク情報（存在する場合）を含む Vec<ScrapedItem> を返す
    ///
    /// # 例
    /// HTML内の段落や見出し、リンクを抽出する場合:
    ///     selector_str = "p, h1, h2, h3, h4, h5, h6, a"
    pub async fn scrape(
        &self,
        url: &str,
        selector_str: &str,
    ) -> Result<ScrapedData, Box<dyn Error + Send + Sync>> {
        // URLの妥当性チェック
        let url = Url::parse(url)?;

        // HTTPリクエストを実行し、ページのHTMLを取得
        let response = self.client.get(url).send().await?.error_for_status()?;
        let body = response.text().await?;

        // HTMLのパース
        let document = Html::parse_document(&body);

        // CSSセレクターをコンパイル
        let selector = Selector::parse(selector_str)
            .map_err(|e| format!("Invalid CSS selector: {}", e))?;

        // 指定されたセレクターに合致するすべての要素のテキストとリンクを収集
        let items: Vec<ScrapedItem> = document
            .select(&selector)
            .map(|element| {
                // 要素内のテキストを連結し、split_whitespace()で余分な空白を除去
                let raw_text = element.text().collect::<Vec<_>>().join(" ");
                let text = raw_text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                // 対象が <a> タグの場合は href 属性を取得
                let link = if element.value().name() == "a" {
                    element.value().attr("href").map(|s| s.to_string())
                } else {
                    None
                };
                ScrapedItem { text, link }
            })
            .filter(|item| !item.text.is_empty())
            .collect();

        Ok(ScrapedData { items })
    }
}

/// AI Functionとして利用するための `Tool` トレイト実装
impl Tool for WebScraper {
    fn def_name(&self) -> &str {
        "web_scraper"
    }

    fn def_description(&self) -> &str {
        "Scrapes a webpage and extracts content based on a provided CSS selector. \
         The CSS selector specifies which elements' text and links (if available) should be returned. \
         For example, 'p, h1, h2, h3, h4, h5, h6, a' selects paragraphs, headings, and links."
    }

    fn def_parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL of the webpage to scrape."
                },
                "selector": {
                    "type": "string",
                    "description": "The CSS selector to extract the desired elements. \
                                   For example, 'p, h1, h2, h3, h4, h5, h6, a' will extract paragraphs, headings, and links."
                }
            },
            "required": ["url", "selector"]
        })
    }

    fn run(&self, args: serde_json::Value) -> Result<String, String> {
        println!("Web scraper {:?}", args);
        // パラメータの取得
        let url = args.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'url' parameter".to_string())?
            .to_string();
        let selector = args.get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'selector' parameter".to_string())?
            .to_string();

        let web_scraper = self.clone();

        // 新しいスレッド上で非同期処理を実行
        let result = std::thread::spawn(move || {
            // このスレッドで新たにTokioランタイムを作成
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(web_scraper.scrape(&url, &selector))
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())?
        .map_err(|e| format!("Scrape error: {}", e))?;

        // JSON形式で出力（余分な空白も含まず、正しいJSONとしてパース可能）
        serde_json::to_string(&result)
            .map_err(|e| format!("Serialization error: {}", e))
    }
}
