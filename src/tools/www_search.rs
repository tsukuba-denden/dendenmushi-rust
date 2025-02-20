use playwright::Playwright;
use urlencoding::encode;

use super::web_scraper::ScraperError;

/// 検索結果のうち、hover‑url 属性を持つ要素のテキスト（題名）と属性値（リンク）を保持する構造体
#[derive(Debug, Clone)]
pub struct BingSearchResult {
    pub title: String,
    pub link: String, // hover‑url 属性の値
}

/// Bing の検索結果ページから、hover‑url 属性を持つ `<a>` 要素を抽出するスクレイパー
pub struct BingSearchScraper {
    playwright: Playwright,
}

impl BingSearchScraper {
    /// 新しいインスタンスを生成する
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let playwright = Playwright::initialize().await?;
        Ok(BingSearchScraper { playwright })
    }

    /// 指定したクエリで Bing 検索を実行し、hover‑url 属性を持つ `<a>` 要素から題名とリンクを取得する
    pub async fn search(&self, query: &str) -> Result<Vec<BingSearchResult>, Box<dyn std::error::Error>> {
        // クエリを URL エンコードして Bing の検索 URL を生成
        let encoded_query = encode(query);
        let url = format!("https://www.bing.com/search?q={}", encoded_query);

        // ブラウザ起動 (headless モードではなく表示状態にしてデバッグ可能)
        let browser = self.playwright
            .chromium()
            .launcher()
            .headless(false)
            .args(&vec![
                String::from("--enable-features=BlockInsecurePrivateNetworkRequests"),
                String::from("--disable-file-system"),
                String::from("--disable-popup-blocking"),
                String::from("--disable-web-security"),
                String::from("--disable-webgl"),
                String::from("--disable-webrtc"),
                String::from("--disable-camera"),
                String::from("--disable-microphone"),
                String::from("--disable-media-source"),
                String::from("--host-resolver-rules=MAP localhost 127.255.255.255"),
            ])
            .launch()
            .await
            .map_err(|_| ScraperError::LaunchError)?;
            
        let context = browser
            .context_builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko, OBSERVER) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .await
            .map_err(|_| ScraperError::ContextError)?;
        let page = context.new_page().await.map_err(|_| ScraperError::PageError)?;

        // 任意の初期化スクリプト（stealth対策など）
        page.add_init_script(include_str!("stealth.min.js"))
            .await
            .map_err(|_| ScraperError::ScriptError)?;

        // 指定の URL に移動 (タイムアウトは 10秒)
        page.goto_builder(&url)
            .timeout(10000.0)
            .goto()
            .await
            .map_err(|_| ScraperError::NetworkError)?;
        
        // まず、hover‑url 属性を持つ `<a>` 要素が存在するのを待機
        page.wait_for_selector_builder("a[hover-url]")
            .timeout(10000000.0)
            .wait_for_selector()
            .await
            .map_err(|_| ScraperError::TimeoutError)?;

        // ページ内のすべての a[hover-url] 要素を取得
        let elements = page.query_selector_all("a[hover-url]").await?;
        let mut results = Vec::new();

        for element in elements {
            let title = element.inner_text().await?;
            let link = element.get_attribute("hover-url").await?.unwrap_or_default();
            results.push(BingSearchResult { title, link });
        }

        browser.close().await?;
        Ok(results)
    }
}