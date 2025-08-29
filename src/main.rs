use std::sync::Arc;
use dashmap::DashMap;
mod agent;
mod handler;

use handler::Handler;

use call_agent::chat::{api::{UserLocation, WebSearchOptions}, client::{ModelConfig, OpenAIClient}};
use observer::{prefix::{ASSISTANT_NAME, DISCORD_TOKEN, ENABLE_BROWSER_TOOL, ENABLE_GET_TIME_TOOL, ENABLE_IMAGE_CAPTIONER_TOOL, ENABLE_MEMORY_TOOL, ENABLE_WEB_DEPLOY_TOOL, MAIN_MODEL_API_KEY, MAIN_MODEL_ENDPOINT, MODEL_GENERATE_MAX_TOKENS, MODEL_NAME}, tools::{browsing_worker::BrowsingWorker, get_time::GetTime, image_captioner::ImageCaptionerTool, memory::MemoryTool, web_deploy::WebDeploy, web_scraper::Browser}}; // use tools::memory::MemoryTool;

use serenity::model::prelude::*;
use serenity::prelude::*;
use log::error;
use regex::Regex;

use reqwest::Client as ReqwestClient;
use std::io::Cursor;
use image::{codecs::gif::GifDecoder, ImageReader, AnimationDecoder, DynamicImage, GenericImageView, ImageFormat, RgbaImage};
use base64::{engine::general_purpose, Engine as _};

async fn fetch_and_encode_images(urls: &[String]) -> Vec<String> {
    println!("fetch_and_encode_images: {:?}", urls);
    // 拡張子チェック＆クエリ対応
    let ext_re = Regex::new(r"(?i)\.(png|jpe?g|gif|webp)(?:[?#].*)?$").unwrap();
    // パラメータなし画像URLを即取得する正規表現
    let strict_ext_re = Regex::new(r"(?i)\.(png|jpe?g|gif|webp)$").unwrap();
    let client = ReqwestClient::new();
    let mut total_bytes = 0u64;
    let mut out = Vec::new();

    for url in urls.iter().filter(|u| ext_re.is_match(u)) {
        // パラメータなし URL は問答無用でオリジナルを取得
        if strict_ext_re.is_match(url) {
            if let Ok(resp) = client.get(url).send().await {
                if let Ok(bytes) = resp.bytes().await {
                    // 拡張子から MIME を決定
                    let ext = strict_ext_re
                        .captures(url)
                        .and_then(|c| c.get(1))
                        .unwrap()
                        .as_str()
                        .to_lowercase();
                    let mime = match ext.as_str() {
                        "png"  => "image/png",
                        "jpg" | "jpeg" => "image/jpeg",
                        "gif"  => "image/gif",
                        "webp" => "image/webp",
                        _      => "application/octet-stream",
                    };
                    out.push(format!("data:{};base64,{}", mime, general_purpose::STANDARD.encode(&bytes)));
                }
            }
            continue;
        }
        let ext = ext_re.captures(url).and_then(|c| c.get(1)).unwrap().as_str().to_lowercase();
        // HEAD でサイズチェック
        let len = client.head(url).send().await
            .ok()
            .and_then(|r| r.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse().ok()))
            .unwrap_or(0);
        if len == 0 || len > 20 * 1024 * 1024 || total_bytes + len > 50 * 1024 * 1024 {
            continue;
        }
        // GET してバイト列取得
        let bytes = match client.get(url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        // 解像度チェック
        let reader = match ext.as_str() {
            "gif" => {
                let decoder = match GifDecoder::new(Cursor::new(&bytes)) {
                    Ok(decoder) => decoder,
                    Err(_) => continue,
                };
                let mut frames = decoder.into_frames();
        
                // Frame を取り出し
                let frame = match frames.next() {
                    Some(Ok(frame)) => frame,
                    _ => continue,
                };
        
                // Frame をバッファ（RgbaImage）に変換
                let buf: RgbaImage = frame.into_buffer();
                DynamicImage::ImageRgba8(buf)
            }
            _ => {
                // 通常の画像
                let img = match ImageReader::new(Cursor::new(&bytes)).with_guessed_format() {
                    Ok(reader) => match reader.decode() {
                        Ok(img) => img,
                        Err(e) => {
                            println!("Error decoding image: {:?}", e);
                            continue;
                        }
                    },
                    Err(e) => {
                        println!("Error creating image reader: {:?}", e);
                        continue;
                    }
                };
                // 透過があれば白背景でフラット化
                if img.color().has_alpha() {
                    let (w, h) = img.dimensions();
                    let mut bg = RgbaImage::new(w, h);
                    for (x, y, p) in img.to_rgba8().enumerate_pixels() {
                        let alpha = p.0[3] as f32 / 255.0;
                        let inv = 1.0 - alpha;
                        let r = (p[0] as f32 * alpha + 255.0 * inv) as u8;
                        let g = (p[1] as f32 * alpha + 255.0 * inv) as u8;
                        let b = (p[2] as f32 * alpha + 255.0 * inv) as u8;
                        bg.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                    }
                    DynamicImage::ImageRgba8(bg)
                } else {
                    img
                }
            }
        };
        // 解像度を調整（長辺>2000なら縮小、短辺<512なら拡大）
        let (w, h) = reader.dimensions();
        let mut img = reader;
        // 長辺が2000pxを超える場合は縮小
        if img.dimensions().0.max(img.dimensions().1) > 2000 {
            let long = img.dimensions().0.max(img.dimensions().1) as f32;
            let scale = 2000.0 / long;
            img = img.resize(
                (w as f32 * scale) as u32,
                (h as f32 * scale) as u32,
                image::imageops::FilterType::Lanczos3,
            );
        }
        // 短辺が512px未満の場合は拡大
        if img.dimensions().0.min(img.dimensions().1) < 512 {
            let (w2, h2) = img.dimensions();
            let short = w2.min(h2) as f32;
            let scale = 512.0 / short;
            img = img.resize(
                (w2 as f32 * scale) as u32,
                (h2 as f32 * scale) as u32,
                image::imageops::FilterType::Lanczos3,
            );
        }
        // PNGで再エンコード → data URL
        let mut buf = Vec::new();
        if img
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .is_err()
        {
            continue;
        }
        total_bytes += len;
        out.push(format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&buf)));
    }

    out
}



#[tokio::main]
async fn main() {
    // ロガーの初期化
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .filter_module("serenity", log::LevelFilter::Off) // serenityクレートのログを除外
        .filter_module("reqwest", log::LevelFilter::Off) // reqwestクレートのログを除外
        .filter_module("hyper", log::LevelFilter::Off) // hyperクレートのログを除外
        .filter_module("rustls", log::LevelFilter::Off) // rustlsクレートのログを除外
        .filter_module("h2", log::LevelFilter::Off) // h2クレートのログを除外
        .filter_module("tungstenite", log::LevelFilter::Off) // tungsteniteクレートのログを除外
        .filter_module("tracing", log::LevelFilter::Off) // tracingクレートのログを除外
        .filter_module("html5ever", log::LevelFilter::Off) // html5everクレートのログを除外
        .filter_module("selectors", log::LevelFilter::Off) // selectorsクレートのログを除外
        .filter_module("playwright", log::LevelFilter::Off) // markup5everクレートのログを除外
        .init();

    // Discord Bot のトークンを取得
    let token = *DISCORD_TOKEN;

    // モデル設定
    let conf = ModelConfig {
        model: MODEL_NAME.to_string(),
        model_name: Some(ASSISTANT_NAME.to_string()),
        parallel_tool_calls: None,
        temperature: None,
        max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
        reasoning_effort: Some("low".to_string()),
        presence_penalty: None,
        strict: Some(false),
        top_p: Some(1.0),
        web_search_options: None,
    };

    // 基本となる OpenAIClient を生成し、ツールを定義
    let mut base_client = OpenAIClient::new(
        *MAIN_MODEL_ENDPOINT,
        Some(*MAIN_MODEL_API_KEY),
    );


    if *ENABLE_BROWSER_TOOL {
        base_client.def_tool(Arc::new(Browser::new()));
    }
    if *ENABLE_MEMORY_TOOL {
        base_client.def_tool(Arc::new(MemoryTool::new()));
    }
    if *ENABLE_GET_TIME_TOOL {
        base_client.def_tool(Arc::new(GetTime::new()));
    }
    if *ENABLE_WEB_DEPLOY_TOOL {
        let web_deploy = Arc::new(WebDeploy::new().await);
        web_deploy.start_server("0.0.0.0:80".to_string());
        base_client.def_tool(web_deploy);
    }
    if *ENABLE_IMAGE_CAPTIONER_TOOL {
        base_client.def_tool(Arc::new(
            ImageCaptionerTool::new({

                let mut c = OpenAIClient::new(
                    *MAIN_MODEL_ENDPOINT,
                    Some(*MAIN_MODEL_API_KEY)
                );
                c.set_model_config(&ModelConfig {
                    model: "gemini-2.5-flash".to_string(),
                    model_name: Some("image_captioner".to_string()),
                    parallel_tool_calls: None,
                    temperature: None,
                    max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                    reasoning_effort: Some("low".to_string()),
                    presence_penalty: None,
                    strict: Some(false),
                    top_p: Some(1.0),
                    web_search_options: None,
                });
                c
            })
        ));
    }
    base_client.def_tool(Arc::new(
        BrowsingWorker::new({
            let mut c = OpenAIClient::new(
                *MAIN_MODEL_ENDPOINT,
                Some(*MAIN_MODEL_API_KEY)
            );
            c.set_model_config(&ModelConfig {
                model: "gemini-1.5-flash".to_string(),
                model_name: Some("browsing_worker".to_string()),
                parallel_tool_calls: None,
                temperature: None,
                max_completion_tokens: Some(*MODEL_GENERATE_MAX_TOKENS as u64),
                reasoning_effort: None,
                presence_penalty: None,
                strict: Some(false),
                top_p: None,
                web_search_options: Some(WebSearchOptions {
                    search_context_size: None,
                    user_location: UserLocation {
                        country: Some("JP".to_string()),
                        region: None,
                        city: None,
                        timezone: None,
                    }
                })
            });
            c
        })
        )
    );
    base_client.set_model_config(&conf);
    let base_client = Arc::new(base_client);

    let channels = DashMap::new();


    // Bot のインテント設定（MESSAGE_CONTENT を含む）
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
    let handler = Handler {
        base_client: base_client.clone(),
        channels: channels.clone(),
        channels_conf: DashMap::new(),
        user_configs: DashMap::new(),
    };
    handler.load();
    let mut client = Client::builder(&token, intents)
        .event_handler(handler)
        .await
        .expect("Error creating client");

    if let Err(e) = client.start().await {
        error!("Client error: {:?}", e);
    }
}