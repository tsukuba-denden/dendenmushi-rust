use serde::{Serialize, Deserialize};
use std::error::Error;
use tokio;

use super::web_scraper::WebScraper;

#[derive(Debug, Serialize, Deserialize)]
pub struct BingSearchResult {
    pub title: String,
    pub link: String,
    pub snippet: Option<String>,
}

#[derive(Clone)]
pub struct BingSearch {
    scraper: WebScraper,
}

impl BingSearch {
    /// 新しい `BingSearch` インスタンスを作成
    pub fn new() -> Self {
        BingSearch {
            scraper: WebScraper::new(),
        }
    }

    /// Bing検索を実行し、結果を取得する
    pub async fn search(&self, query: &str) -> Result<Vec<BingSearchResult>, Box<dyn Error>> {
        let encoded_query = urlencoding::encode(query);
        let bing_url = format!("https://www.bing.com/search?q={}", encoded_query);

        // Bing の検索結果ページからデータを取得
        let scraped_data = self.scraper.scrape_playwright(&bing_url, "li.b_algo").await?;

        let mut results = Vec::new();

        for item in scraped_data.items {
            // テキストが空でないかチェック
            if !item.text.is_empty() {
                if let Some(link) = item.link {
                    if WebScraper::is_safe_url(&link) {
                        results.push(BingSearchResult {
                            title: item.text,
                            link,
                            snippet: None, // スニペットを取得する場合は追加処理が必要
                        });
                    }
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests { 
    use super::*;

    #[tokio::test]
    async fn test_bing_search() {
        let bing_search = BingSearch::new();
        let results = bing_search.search("Rust programming").await.unwrap();

        assert!(!results.is_empty());
        for result in results {
            println!("{:?}", result);
        }
    }
}