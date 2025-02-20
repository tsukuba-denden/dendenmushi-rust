use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use call_agent::chat::function::Tool;
use chrono::{Datelike, Local};
use log::debug;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::{fs, thread};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::prefix::DOMAIN;

const BASE_DIR: &str = "./data";

#[derive(Clone)]
pub struct WebDeploy {
    file_map: Arc<RwLock<HashMap<String, PathBuf>>> // 記事キー → ファイルパス
}

async fn get_article(data: web::Data<Arc<RwLock<HashMap<String, PathBuf>>>>, key: web::Path<String>) -> impl Responder {
    let file_map = data.get_ref();
    let key_str = key.into_inner();
    let file_map_read = file_map.read().await;
    if let Some(path) = file_map_read.get(&key_str) {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => HttpResponse::Ok().body(content),
            Err(e) => {
                debug!("Error loading article {}: {}", key_str, e);
                HttpResponse::InternalServerError().body("Failed loading article")
            },
        }
    } else {
        HttpResponse::NotFound().body("Article not found")
    }
}

async fn view_article(key: web::Path<String>) -> impl Responder {
    let key_str = key.into_inner();
    let html_template = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Article Viewer</title>
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.7.0/styles/github-dark.min.css">
    <script src="https://cdnjs.cloudflare.com/ajax/libs/marked/4.2.12/marked.min.js"></script>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.7.0/highlight.min.js"></script>
    <style>
        /* ダークテーマ */
        body {
            font-family: Arial, sans-serif;
            background-color: #121212;
            color: #e0e0e0;
            margin: 0;
            padding: 20px;
        }
        h1 {
            text-align: center;
            color: #ffffff;
        }
        #content {
            max-width: 800px;
            margin: auto;
            background-color: #1e1e1e;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 4px 8px rgba(0, 0, 0, 0.2);
        }
        pre {
            padding: 10px;
            background-color: #2d2d2d;
            border-radius: 5px;
            overflow-x: auto;
        }
        code {
            font-family: "Fira Code", monospace;
        }
        a {
            color: #64b5f6;
            text-decoration: none;
        }
        a:hover {
            text-decoration: underline;
        }
    </style>
</head>
<body>
    <h1>Observer Article Viewer</h1>
    <div id="content">Loading...</div>

    <script>
        document.addEventListener("DOMContentLoaded", async () => {
            const key = "{{key}}";
            try {
                const response = await fetch(`/articles/${key}`);
                const text = await response.text();
                
                // Markdown を HTML に変換
                const html = marked.parse(text);
                document.getElementById('content').innerHTML = html;

                // シンタックスハイライト適用
                document.querySelectorAll('pre code').forEach((block) => {
                    hljs.highlightElement(block);
                });
            } catch (error) {
                console.error('Error fetching article:', error);
                document.getElementById('content').innerHTML = "<p>Error loading article.</p>";
            }
        });
    </script>
</body>
</html>

"#;
    let html_content = html_template.replace("{{key}}", &key_str);
    HttpResponse::Ok().content_type("text/html").body(html_content)
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
                            for articles in fs::read_dir(m_path).expect("Failed to read directory") {
                                let articles = articles.expect("Failed to read directory entry");
                                let articles_path = articles.path();
                                if articles_path.is_file() {
                                    let relative_path = articles_path.strip_prefix(BASE_DIR).unwrap();
                                    let key = relative_path.to_string_lossy().split(&['/', '\\'][..]).collect::<Vec<&str>>()[2].to_string();
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
        let content = fs::read_to_string(path).map(|s| s.to_string()).map_err(|_| "Failed loading article")?;
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
        
        file_map.write().await.insert(key.to_string(), key_path.clone());
        Ok(format!("https://{}/v/{}", *DOMAIN, key))
    }



    pub fn start_server(&self, bind: String) {
        let map = Arc::clone(&self.file_map);
        thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                HttpServer::new(move || {
                    App::new()
                        .app_data(web::Data::new(Arc::clone(&map)))
                        .route("/articles/{key}", web::get().to(get_article))
                        .route("/v/{key}", web::get().to(view_article))
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
        "A tool to deploy articles to the web
        The article is expected to be of substantial length.
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
                    "description": "Key of the article"
                },
                "content": {
                    "type": "string",
                    "description": "Content of the article",
                }
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
        let result = thread::spawn(move || {
            let rt = Runtime::new().expect("Failed to create runtime");

            let action = args_clone.get("action")
                .and_then(|v| v.as_str())
                .ok_or("Missing or invalid 'action' parameter")?;
            let key = args_clone.get("key")
                .and_then(|v| v.as_str())
                .ok_or("Missing or invalid 'key' parameter")?;
            match action {
                "get" => rt.block_on(async { 
                    self_clone.get_article(key).await
                }),
                "create" => rt.block_on(async { 
                    let content = args_clone.get("content")
                        .and_then(|v| v.as_str())
                        .ok_or("Missing or invalid 'content' parameter")?;
                    self_clone.create_article(key, content).await
                }),
                "found" => rt.block_on(async { 
                    let found = self_clone.found_article(key).await;
                    Ok(found.to_string())
                }),
                _ => Err("Invalid action".to_string()),
            }
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())?;

        result
    }   
}
