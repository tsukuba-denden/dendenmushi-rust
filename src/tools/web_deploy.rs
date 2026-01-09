use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use call_agent::chat::function::Tool;
use chrono::{Datelike, Local};
use log::debug;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, thread};
use tokio::runtime::Runtime;
use tokio::sync::RwLock;

use crate::prefix::DOMAIN;
use actix_web::HttpRequest;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const BASE_DIR: &str = "./data";

#[derive(Clone)]
pub struct WebDeploy {
    file_map: Arc<RwLock<HashMap<String, PathBuf>>>, // 記事キー → ファイルパス
}

async fn list_articles_by_month(
    path: web::Path<(String, String)>,
    query: web::Query<HashMap<String, String>>,
) -> impl Responder {
    let (year, month) = path.into_inner();
    let normalized_month = month.trim_start_matches('0');
    let base_path = format!("{}/{}/{}", BASE_DIR, year, normalized_month);
    let page: usize = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    let per_page: usize = query
        .get("per_page")
        .and_then(|p| p.parse().ok())
        .unwrap_or(10);

    let mut articles = Vec::new();
    if let Ok(files) = fs::read_dir(&base_path) {
        for file_entry in files.flatten() {
            if let Some(article_name) = file_entry.file_name().to_str() {
                articles.push(article_name.to_string());
            }
        }
    }

    let start = (page - 1) * per_page;
    let end = start + per_page;
    let paginated_articles = articles[start.min(articles.len())..end.min(articles.len())].to_vec();

    HttpResponse::Ok().json(serde_json::json!({ "articles": paginated_articles }))
}

async fn get_article_raw(
    path: web::Path<(String, String, String)>,
    req: HttpRequest,
) -> impl Responder {
    let (year, month, article) = path.into_inner();
    let normalized_month = month.trim_start_matches('0');
    let file_path = format!("{}/{}/{}/{}", BASE_DIR, year, normalized_month, article);

    if let Ok(mut file) = File::open(&file_path) {
        let metadata = file.metadata().ok();
        let file_size = metadata.map(|m| m.len()).unwrap_or(0);

        if let Some(range_header) = req.headers().get("Range")
            && let Ok(range) = range_header.to_str()
                && let Some(range) = range.strip_prefix("bytes=") {
                    let parts: Vec<&str> = range.split('-').collect();
                    if let (Some(start_str), Some(end_str)) = (parts.first(), parts.get(1))
                        && let (Ok(start), Ok(end)) =
                            (start_str.parse::<u64>(), end_str.parse::<u64>())
                            && start < file_size && end < file_size {
                                let mut buffer = vec![0; (end - start + 1) as usize];
                                file.seek(SeekFrom::Start(start)).ok();
                                file.read_exact(&mut buffer).ok();
                                return HttpResponse::PartialContent()
                                    .insert_header((
                                        "Content-Range",
                                        format!("bytes {}-{}/{}", start, end, file_size),
                                    ))
                                    .body(buffer);
                            }
                }

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).ok();
        return HttpResponse::Ok().body(buffer);
    }

    HttpResponse::NotFound().body("記事が見つかりません")
}

async fn root_page() -> impl Responder {
    let file_path = "./data/index.html";
    if let Ok(content) = fs::read_to_string(file_path) {
        HttpResponse::Ok().content_type("text/html").body(content)
    } else {
        HttpResponse::NotFound().body("ルートページが見つかりません")
    }
}

async fn favicon() -> impl Responder {
    let file_path = "./data/favicon.ico";
    if let Ok(mut file) = File::open(file_path) {
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).ok();
        return HttpResponse::Ok().content_type("image/x-icon").body(buffer);
    }
    HttpResponse::NotFound().body("Favicon not found")
}

impl WebDeploy {
    pub async fn new() -> Self {
        let file_map = Arc::new(RwLock::new(HashMap::new()));
        let base_dir = Path::new(BASE_DIR);
        if base_dir.exists() {
            for y in fs::read_dir(base_dir).expect("Failed to read directory") {
                let y = y.expect("Failed to read directory entry");
                let y_path = y.path();
                if y_path.is_dir() {
                    for m in fs::read_dir(y_path).expect("Failed to read directory") {
                        let m = m.expect("Failed to read directory entry");
                        let m_path = m.path();
                        if m_path.is_dir() {
                            for articles in fs::read_dir(m_path).expect("Failed to read directory")
                            {
                                let articles = articles.expect("Failed to read directory entry");
                                let articles_path = articles.path();
                                if articles_path.is_file() {
                                    let relative_path =
                                        articles_path.strip_prefix(BASE_DIR).unwrap();
                                    let key = relative_path
                                        .to_string_lossy()
                                        .split(&['/', '\\'][..])
                                        .collect::<Vec<&str>>()[2]
                                        .to_string();
                                    debug!("loading article: {}", key);
                                    file_map.write().await.insert(key, articles_path.clone());
                                }
                            }
                        }
                    }
                }
            }
        } else {
            fs::create_dir_all(base_dir).expect("Failed to create directory");
        }

        Self { file_map }
    }

    pub async fn get_article(&self, key: &str) -> Result<String, String> {
        let file_map = self.file_map.read().await;
        let path = file_map.get(key).ok_or("Article not found")?;
        let content = fs::read_to_string(path)
            .map(|s| s.to_string())
            .map_err(|_| "Failed loading article")?;
        Ok(content)
    }

    pub async fn found_article(&self, key: &str) -> bool {
        self.file_map.read().await.contains_key(key)
    }

    pub async fn create_article(&self, key: &str, content: &str) -> Result<String, String> {
        let file_map = Arc::clone(&self.file_map);
        let base_dir = Path::new(BASE_DIR);
        let now = Local::now();
        let year = now.year().to_string();
        let month = now.month().to_string();
        let key_path = base_dir.join(&year).join(&month).join(key);

        fs::create_dir_all(key_path.parent().unwrap()).map_err(|_| "Failed to create directory")?;
        fs::write(&key_path, content).map_err(|_| "Failed to write file")?;

        file_map
            .write()
            .await
            .insert(key.to_string(), key_path.clone());
        Ok(format!(
            "https://{}/?view=article&year={}&month={}&article={}",
            *DOMAIN, year, month, key
        ))
    }

    pub fn start_server(&self, bind: String) {
        let map = Arc::clone(&self.file_map);
        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                HttpServer::new(move || {
                    App::new()
                        .app_data(web::Data::new(Arc::clone(&map)))
                        .route("/", web::get().to(root_page))
                        .route(
                            "/articles/{year}/{month}",
                            web::get().to(list_articles_by_month),
                        )
                        .route(
                            "/article/raw/{year}/{month}/{article}",
                            web::get().to(get_article_raw),
                        )
                        .route("/favicon.ico", web::get().to(favicon))
                })
                .bind(bind.as_str())
                .expect("Failed to bind server")
                .run()
                .await
                .expect("Server run failed");
            });
        });
    }
}

impl Tool for WebDeploy {
    fn def_name(&self) -> &str {
        "web_deploy_tool"
    }

    fn def_description(&self) -> &str {
        "A tool to deploy articles to the web.
        The article should be crafted to provide a reading experience of approximately 1 to 5 minutes.
        Also, please provide an appropriate source of information."
    }

    fn def_parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "create", "found"],
                    "description": "Action to perform: 'get' (retrieve an article), 'create' (add a new article), 'found' (check if an article exists)"
                },
                "key": {
                    "type": "string",
                    "description": "Name of the article"
                },
                "content": {
                    "type": "string",
                    "description": "Content of the article",
                },
                "$explain": {
                    "type": "string",
                    "description": "A brief explanation of what you are doing with this tool."
                },
            },
            "required": ["action", "key"],
            "if": {
                "properties": {
                    "action": { "const": "create" }
                }
            },
            "then": {
                "required": ["content"]
            }
        })
    }

    fn run(&self, args: serde_json::Value) -> Result<String, String> {
        let self_clone = self.clone();

        let args_clone = args.clone();
        

        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create runtime");

            let action = args_clone
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or("Missing or invalid 'action' parameter")?;
            let raw_key = args_clone
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("Missing or invalid 'key' parameter")?;
            let sanitized_key: String = raw_key
                .chars()
                .filter(|&c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
                .collect();
            if sanitized_key.is_empty() {
                return Err(
                    "The provided 'key' parameter contains only invalid characters".to_string(),
                );
            }
            let key = sanitized_key;
            match action {
                "get" => rt.block_on(async { self_clone.get_article(&key).await }),
                "create" => rt.block_on(async {
                    let content = args_clone
                        .get("content")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing or invalid 'content' parameter")?;
                    self_clone.create_article(&key, content).await
                }),
                "found" => rt.block_on(async {
                    let found = self_clone.found_article(&key).await;
                    Ok(found.to_string())
                }),
                _ => Err("Invalid action".to_string()),
            }
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())?
    }
}
