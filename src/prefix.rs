use log::warn;
use serde::Deserialize;
use std::fs;
use std::io::Write;

#[derive(Deserialize, Debug)]
pub struct ModelSettings {
    pub model_generate_max_tokens: usize,
    pub main_model_endpoint: String,
    pub main_model_api_key: String,
    pub model_name: String,
}

#[derive(Deserialize, Debug)]
pub struct PromptSettings {
    pub ask_developer_prompt: String,
}

#[derive(Deserialize, Debug)]
pub struct Settings {
    pub assistant_name: String,
    pub max_use_tool_count: usize,
    pub enable_web_deploy_tool: bool,
    pub enable_browser_tool: bool,
    pub enable_memory_tool: bool,
    pub enable_get_time_tool: bool,
    pub enable_image_captioner_tool: bool,
    pub sec_per_rate: usize,
    pub rate_cp: usize,
    pub model: ModelSettings,
    pub prompt: PromptSettings,
    pub discord_token: String,
    pub server_domain: String,
    pub admin_users: Vec<String>,
}

// グローバル変数として設定を保持する
lazy_static::lazy_static! {
    pub static ref CONFIG: Settings = Settings::new("config.json");
    pub static ref ASSISTANT_NAME: &'static str = &CONFIG.assistant_name;
    pub static ref MAX_USE_TOOL_COUNT: usize = CONFIG.max_use_tool_count;
    pub static ref MODEL_GENERATE_MAX_TOKENS: usize = CONFIG.model.model_generate_max_tokens;
    pub static ref MAIN_MODEL_ENDPOINT: &'static str = &CONFIG.model.main_model_endpoint;
    pub static ref MAIN_MODEL_API_KEY: &'static str = &CONFIG.model.main_model_api_key;
    pub static ref ENABLE_WEB_DEPLOY_TOOL: bool = CONFIG.enable_web_deploy_tool;
    pub static ref ENABLE_BROWSER_TOOL: bool = CONFIG.enable_browser_tool;
    pub static ref ENABLE_MEMORY_TOOL: bool = CONFIG.enable_memory_tool;
    pub static ref ENABLE_GET_TIME_TOOL: bool = CONFIG.enable_get_time_tool;
    pub static ref ENABLE_IMAGE_CAPTIONER_TOOL: bool = CONFIG.enable_image_captioner_tool;
    pub static ref ASK_DEVELOPER_PROMPT: &'static str = &CONFIG.prompt.ask_developer_prompt;
    pub static ref DISCORD_TOKEN: &'static str = &CONFIG.discord_token;
    pub static ref DOMAIN: &'static str = &CONFIG.server_domain;
    pub static ref MODEL_NAME: &'static str = &CONFIG.model.model_name;
    pub static ref SEC_PER_RATE: usize = CONFIG.sec_per_rate;
    pub static ref RATE_CP: usize = CONFIG.rate_cp;
    pub static ref ADMIN_USERS: Vec<String> = CONFIG.admin_users.clone();
}

impl Settings {
    pub fn new(config_path: &str) -> Self {
        let config_data = fs::read_to_string(config_path).unwrap_or_else(|_| {
            let default_config = r#"
{
    "assistant_name": "observer",
    "max_use_tool_count": 5,
    "enable_web_deploy_tool": true,
    "enable_browser_tool": true,
    "enable_memory_tool": true,
    "enable_get_time_tool": true,
    "enable_image_captioner_tool": true,
    "sec_per_rate": 30,
    "rate_cp": 60,
    "admin_users": [],
    "model": {
        "model_generate_max_tokens": 4096,
        "main_model_endpoint": "https://api.openai.com/v1/",
        "main_model_api_key": "YOUR_API_KEY",
        "model_name": "gpt-4o-mini",
    },
    "prompt": {
        "ask_developer_prompt": "重要: あなたはDiscord上で活動しているObserverという名前の人格で自然に会話します\ntool callを活用してください\n依頼は応答で返し、どうしても詳細な説明が必要な場合のみ質問者に尋ねてください\n適切な場合には、(´-ω-`) のような絵文字を使って感情を表現してください\nあまり'!'を使わないようにしてください\n重要: 一語だけの返答が可能な場合は、一語で応答してください（例: 'うん', 'そう', 'は？', 'えぇ...', '??????'）\n重要: バランスのとれた、知的に引き込む、かつ技術的に情報に精通した態度を保ってください\nカジュアルな会話を行います。会話の流れを意識するように\n顔文字やスラングを適度に使い、返答は短めでテンポよくします\n静かでかわいい性格\n最も重要: 周りの人のしゃべり方などを真似するのがもっとも効果的\n応答にメタデータを含めないでください\nネットを使った場合は情報源を示すようにしなさい\n応答が長くなったり、説明がとても長くなる もしくは説明がまとめれたときはweb_deploy_toolを使うと良いでしょう\n記事を書いたらどんな記事を書いたかかるくmemoryしておくとよいでしょう"
    },
    "discord_token": "YOUR_API_KEY",
    "server_domain": "dev.371tti.net"
}
            "#;
            let mut file = fs::File::create(config_path).expect("Unable to create config file");
            file.write_all(default_config.as_bytes()).expect("Unable to write default config file");
            warn!("Config file not found. Creating a new one with default settings. please edit 'config.json' file");
            default_config.to_string()
        });
        serde_json::from_str(&config_data).expect("Unable to parse config file")
    }
}
