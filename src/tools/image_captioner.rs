use std::{collections::VecDeque, io::Cursor};

use call_agent::chat::{client::OpenAIClient, function::Tool, prompt::{Message, MessageContext, MessageImage}};
use image::{codecs::gif::GifDecoder, AnimationDecoder, DynamicImage, GenericImageView, ImageReader, RgbaImage};
use reqwest::Client;
use serde_json::Value;
use tokio::runtime::Runtime;
use base64::{engine::general_purpose, Engine as _};

/// **テキストの長さを計算するツール**
pub struct ImageCaptionerTool {
    pub model: OpenAIClient,
}

impl ImageCaptionerTool {
    pub fn new(model: OpenAIClient) -> Self {
        Self { model }
    }
    
    pub async fn fetch_and_encode_image(url: &str) -> Option<String> {
        // Content-Type ヘッダーで MIME タイプとサイズを判定
        let client = Client::new();
        let head = client.head(url).send().await.ok()?;
        // MIME タイプ確認
        let ct = head
            .headers()
            .get(reqwest::header::CONTENT_TYPE)?
            .to_str()
            .ok()?;
        if !ct.starts_with("image/") {
            return None;
        }
        let mime = ct.split(';').next().unwrap();
        // サイズチェック（20MB上限）
        let len = head
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
            .unwrap_or(0);
        if len == 0 || len > 20 * 1024 * 1024 {
            return None;
        }
        // GET してバイト列取得
        let bytes = client.get(url).send().await.ok()?.bytes().await.ok()?;

        // 画像デコード＋透過処理／GIFは最初のフレームを抽出
        let img: DynamicImage = if mime == "image/gif" {
            let decoder = GifDecoder::new(Cursor::new(&bytes)).ok()?;
            let mut frames = decoder.into_frames();
            let frame = frames.next()?.ok()?;
            let buf: RgbaImage = frame.into_buffer();
            DynamicImage::ImageRgba8(buf)
        } else {
            let adyn = ImageReader::new(Cursor::new(&bytes))
                .with_guessed_format().ok()?
                .decode().ok()?;
            // 透過があれば白背景に合成
            if adyn.color().has_alpha() {
                let (w, h) = adyn.dimensions();
                let mut bg = RgbaImage::new(w, h);
                for (x, y, p) in adyn.to_rgba8().enumerate_pixels() {
                    let alpha = p.0[3] as f32 / 255.0;
                    let inv = 1.0 - alpha;
                    let r = (p[0] as f32 * alpha + 255.0 * inv) as u8;
                    let g = (p[1] as f32 * alpha + 255.0 * inv) as u8;
                    let b = (p[2] as f32 * alpha + 255.0 * inv) as u8;
                    bg.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                }
                DynamicImage::ImageRgba8(bg)
            } else {
                adyn
            }
        };
    
        // 解像度を調整（長辺>2000なら縮小、短辺<512なら拡大）
        let (w, h) = img.dimensions();
        let mut img = img;
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
            .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .is_err()
        {
            return None;
        }
        // PNG に再エンコード→data URL
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png).ok()?;
        Some(format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&buf)))
    }
}

impl Tool for ImageCaptionerTool {
    fn def_name(&self) -> &str {
        "image_captioner"
    }

    fn def_description(&self) -> &str {
        "Generate a caption for an image. Can use natural language query to get the caption of the image. This can only analyze information to the extent of a summary of the image."
    }

    fn def_parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Input text to calculate its length."
                },
                "query": {
                    "type": "string",
                    "description": "Input natural language query to get the caption of the image.ex 'write out all the information in the image.'"
                },
                "$explain": {
                    "type": "string",
                    "description": "A brief explanation of what you are doing with this tool."
                },
            },
            "required": ["url", "query"]
        })
    }
    fn run(&self, args: Value) -> Result<String, String> {
        // JSONから"url"キーを取得して String 化
        let url = args["url"].as_str()
            .ok_or_else(|| "Missing 'url' parameter".to_string())?
            .to_string();
        let query = args["query"].as_str()
            .ok_or_else(|| "Missing 'query' parameter".to_string())?
            .to_string();

        // self.model を Clone（Arc<Model> なら Arc::clone(&self.model)）
        let model = self.model.clone();

        // スレッドに渡すのは url, query, model のみ
        let result = std::thread::spawn(move || -> Result<String, String> {
            let rt = Runtime::new().expect("Failed to create runtime");

            // 画像を取得してエンコード
            let data_url = rt.block_on(async {
                Self::fetch_and_encode_image(&url).await
            }).ok_or_else(|| "Failed to fetch and encode image".to_string())?;

            let messages = VecDeque::from(vec![
                Message::User {
                    name: Some("observer".to_string()),
                    content: vec![
                        MessageContext::Text(query.clone()),
                        MessageContext::Image(MessageImage { url: data_url, detail: None }),
                    ],
                }
            ]);

            // モデルに投げる
            let res = rt.block_on(async {
                model.send(&messages, None).await
            }).map_err(|_| "Failed to generate caption".to_string())?;

            // レスポンス解析
            let caption = res
                .response
                .choices
                .ok_or_else(|| "Missing choices in response".to_string())?
                .get(0)
                .ok_or_else(|| "No choice available".to_string())?
                .message
                .content
                .clone()
                .ok_or_else(|| "No content in message".to_string())?;
            Ok(caption)
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())??;

        // JSONで結果を返す
        Ok(serde_json::json!({ "caption": result }).to_string())
    }
}