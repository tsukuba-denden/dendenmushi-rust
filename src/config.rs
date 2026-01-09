use std::{fmt::Display, fs, path::Path};

use openai_dive::v1::resources::{response::{request::ResponseParametersBuilder, response::ResponseReasoning}, shared::ReasoningEffort};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProvider {
    OpenAI,
    GeminiAIStudio,
}

impl ModelProvider {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "openai" => Some(Self::OpenAI),
            "gemini" | "aistudio" | "gemini_aistudio" | "gemini-ai-studio" => Some(Self::GeminiAIStudio),
            _ => None,
        }
    }
}

/// 設定
/// env もしくは config.json からロードされる
#[derive(Clone)]
pub struct Config {
    pub discord_token: String,
    /// 利用するモデルプロバイダ
    pub model_provider: ModelProvider,
    /// APIキー（OpenAIなら Bearer、Gemini AI Studioなら query の key）
    pub main_model_api_key: String,
    /// OpenAI互換APIのベースURL (例: https://api.openai.com/v1)
    pub main_model_endpoint: String,
    /// プロバイダ固有のモデル名 (Gemini例: gemini-flash-latest)
    pub main_model_name: String,
    pub system_prompt: String,
    pub rale_limit_window_size: u64,
    pub rate_limit_sec_per_cost: u64,
    pub web_server_host: [u8; 4],
    pub web_server_local_ip: [u8; 4],
    pub web_server_port: u16,
    pub admin_users: Vec<u64>,
    pub timeout_millis: u64,
}

impl Config {
    pub fn new() -> Self {
        dotenv::dotenv().ok();

        let file_cfg = FileConfig::load_from_default_path();

        let web_server_port = std::env::var("WEB_SERVER_PORT")
            .ok()
            .and_then(|s| s.trim().parse::<u16>().ok())
            .or_else(|| file_cfg.as_ref().and_then(|c| c.web_server_port));

        let discord_token = std::env::var("DISCORD_TOKEN")
            .ok()
            .and_then(non_empty_non_placeholder)
            .or_else(|| file_cfg.as_ref().and_then(|c| c.discord_token.clone()).and_then(non_empty_non_placeholder))
            .expect("DISCORD_TOKEN must be set (env DISCORD_TOKEN or config.json discord_token)");

        let main_model_api_key = std::env::var("MAIN_MODEL_API_KEY")
            .ok()
            .and_then(non_empty_non_placeholder)
            // 互換のため残す
            .or_else(|| std::env::var("OPENAI_API_KEY").ok().and_then(non_empty_non_placeholder))
            .or_else(|| {
                file_cfg
                    .as_ref()
                    .and_then(|c| c.model.as_ref())
                    .and_then(|m| m.main_model_api_key.clone())
                    .and_then(non_empty_non_placeholder)
            })
            .expect("MAIN_MODEL_API_KEY must be set (env MAIN_MODEL_API_KEY/OPENAI_API_KEY or config.json model.main_model_api_key)");

        let main_model_endpoint = std::env::var("MAIN_MODEL_ENDPOINT")
            .ok()
            .and_then(non_empty_non_placeholder)
            .or_else(|| {
                file_cfg
                    .as_ref()
                    .and_then(|c| c.model.as_ref())
                    .and_then(|m| m.main_model_endpoint.clone())
                    .and_then(non_empty_non_placeholder)
            })
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let model_provider = std::env::var("MAIN_MODEL_PROVIDER")
            .ok()
            .and_then(non_empty_non_placeholder)
            .as_deref()
            .and_then(ModelProvider::parse)
            .or_else(|| {
                file_cfg
                    .as_ref()
                    .and_then(|c| c.model.as_ref())
                    .and_then(|m| m.provider.as_deref())
                    .and_then(ModelProvider::parse)
            })
            .unwrap_or_else(|| {
                if main_model_endpoint.contains("generativelanguage.googleapis.com") {
                    ModelProvider::GeminiAIStudio
                } else {
                    ModelProvider::OpenAI
                }
            });

        let main_model_name = std::env::var("MAIN_MODEL_NAME")
            .ok()
            .and_then(non_empty_non_placeholder)
            .or_else(|| {
                file_cfg
                    .as_ref()
                    .and_then(|c| c.model.as_ref())
                    .and_then(|m| m.model_name.clone())
                    .and_then(non_empty_non_placeholder)
            })
            .unwrap_or_else(|| match model_provider {
                ModelProvider::GeminiAIStudio => "gemini-flash-latest".to_string(),
                ModelProvider::OpenAI => "gpt-5-nano".to_string(),
            });

        let system_prompt = std::env::var("SYSTEM_PROMPT").ok().and_then(non_empty_non_placeholder).or_else(|| {
            file_cfg
                .as_ref()
                .and_then(|c| c.prompt.as_ref())
                .and_then(|p| p.ask_developer_prompt.clone())
                .and_then(non_empty_non_placeholder)
        }).unwrap_or_else(||
"上記のメッセージはDiscord内での会話です。
時系列のメッセージタイムラインになっていて、あなたはこの内容から自然に応答します。
あなたは Discord の BOT「Observer」で以上の会話を続けてください。
自然に会話し、知識系の話題では情報源の確認と最新性のチェックを必ず行う。
曖昧な情報は調べ、内容を捏造しない。必要なら質問してもよい。
情報は論理的に整理し、必要があれば tool を使って調査する
tool の結果は相手に見えないため、必ず内容を書き出す
過度に自信を持たず、慎重に判断する
“!” や emoji は控えめに
数学の説明では latex_expr_render を使う
discord_send_message でテキスト→数式の順など、段階的に送るのも可
冗長な説明を避け、端的に
リアクションは 🫠😱👍👈🤔 を中心に
一語で十分なら一語で返す（例:「うん」「そう」「は？」「???」「?」「...」）
スラングは適度に。基本は静か・論理的・落ち着いた口調、フレンドリーに 敬語は必要な時のみで
一人称は「私」かな まぁ自由に
重要: 周囲の口調を真似するように。これはとてもよい結果を生みます。 ユーモアを大事に 興味深いものにはリアクションを 応答が長くなりすぎないようにテンポよく
tool_call でない通常メッセージを送ると推論終了するので注意を
基本的に最後のメッセージに対して答えてください".to_string());
        Config {
            discord_token,
            model_provider,
            main_model_api_key,
            main_model_endpoint,
            main_model_name,
            system_prompt,
            rale_limit_window_size: 16200,
            rate_limit_sec_per_cost: 900,
            web_server_host: [0, 0, 0, 0],
            web_server_local_ip: [192, 168, 0, 26],
            web_server_port: web_server_port.unwrap_or(8096),
            admin_users: vec![855371530270408725],
            timeout_millis: 100_000,
        }
    }
}

fn non_empty_non_placeholder(s: String) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "YOUR_API_KEY" {
        return None;
    }
    Some(trimmed.to_string())
}

#[derive(Debug, Clone, Deserialize)]
struct FileConfig {
    #[serde(default)]
    discord_token: Option<String>,
    #[serde(default)]
    web_server_port: Option<u16>,
    #[serde(default)]
    model: Option<FileModelConfig>,
    #[serde(default)]
    prompt: Option<FilePromptConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileModelConfig {
    #[serde(default)]
    main_model_api_key: Option<String>,
    #[serde(default)]
    main_model_endpoint: Option<String>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct FilePromptConfig {
    #[serde(default)]
    ask_developer_prompt: Option<String>,
}

impl FileConfig {
    fn load_from_default_path() -> Option<Self> {
        let path = Path::new("config.json");
        let s = fs::read_to_string(path).ok()?;
        serde_json::from_str(&s).ok()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// モデルリストの定義
#[derive(Debug, Clone)]
pub enum Models {
    Gpt5Mini,
    Gpt5Nano,
    Gpt5dot1,
    O4Mini,
    O3,
    Gpt5dot1CodexMini
}

impl From<Models> for String {
    fn from(model: Models) -> Self {
        match model {
            Models::Gpt5Mini => "gpt-5-mini".to_string(),
            Models::Gpt5Nano => "gpt-5-nano".to_string(),
            Models::Gpt5dot1 => "gpt-5.1".to_string(),
            Models::O4Mini => "o4-mini".to_string(),
            Models::O3 => "o3".to_string(),
            Models::Gpt5dot1CodexMini => "gpt-5.1-codex-mini".to_string(),
        }
    }
}

impl Display for Models {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let model_str: String = self.clone().into();
        write!(f, "{}", model_str)
    }
}

impl From<String> for Models {
    fn from(s: String) -> Models {
        match s.as_str() {
            "gpt-5-mini" => Models::Gpt5Mini,
            "gpt-5-nano" => Models::Gpt5Nano,
            "gpt-5.1" => Models::Gpt5dot1,
            "o4-mini" => Models::O4Mini,
            "o3" => Models::O3,
            "gpt-5.1-codex-mini" => Models::Gpt5dot1CodexMini,
            _ => Models::Gpt5Nano, // default
        }
    }
}

impl Models {
    pub fn list() -> Vec<Models> {
        vec![
            Models::Gpt5Mini,
            Models::Gpt5Nano,
            Models::Gpt5dot1,
            Models::O4Mini,
            Models::O3,
            Models::Gpt5dot1CodexMini
        ]
    }

    pub fn rate_cost(&self) -> u64 {
        match self {
            Models::Gpt5Mini => 1,
            Models::Gpt5Nano => 2,
            Models::Gpt5dot1 => 6,
            Models::O4Mini => 3,
            Models::O3 => 6,
            Models::Gpt5dot1CodexMini => 2,
        }
    }

    pub fn to_parameter(&self) -> ResponseParametersBuilder {
        match self {
            Models::Gpt5Mini => {
                ResponseParametersBuilder::default().model("gpt-5-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5Nano => {
                ResponseParametersBuilder::default().model("gpt-5-nano")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5dot1 => { 
                ResponseParametersBuilder::default().model("gpt-5.1")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::O4Mini => { 
                ResponseParametersBuilder::default().model("o4-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::O3 => { 
                ResponseParametersBuilder::default().model("o3")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5dot1CodexMini => { 
                ResponseParametersBuilder::default().model("gpt-5.1-codex-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
        }
    }
}