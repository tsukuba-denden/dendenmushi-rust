use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug)]
pub struct ModelSettings {
    pub model_generate_max_tokens: usize,
    pub main_model_endpoint: String,
    pub main_model_api_key: String,
    pub judge_model_endpoint: String,
    pub judge_model_api_key: String,
}

#[derive(Deserialize, Debug)]
pub struct PromptSettings {
    pub ask_developer_prompt: String,
    pub deep_search_developer_prompt: String,
    pub deep_search_generate_prompt: String,
}

#[derive(Deserialize, Debug)]
pub struct Settings {
    pub assistant_name: String,
    pub max_use_tool_count: usize,
    pub model: ModelSettings,
    pub prompt: PromptSettings,
    pub discord_token: String,
    pub server_domain: String,
}

impl Settings {
    pub fn new(config_path: &str) -> Self {
        let config_data = fs::read_to_string(config_path).expect("Unable to read config file");
        serde_json::from_str(&config_data).expect("Unable to parse config file")
    }
}

// グローバル変数として設定を保持する
lazy_static::lazy_static! {
    pub static ref CONFIG: Settings = Settings::new("config.json");
    pub static ref ASSISTANT_NAME: &'static str = &CONFIG.assistant_name;
    pub static ref MAX_USE_TOOL_COUNT: usize = CONFIG.max_use_tool_count;
    pub static ref MODEL_GENERATE_MAX_TOKENS: usize = CONFIG.model.model_generate_max_tokens;
    pub static ref MAIN_MODEL_ENDPOINT: &'static str = &CONFIG.model.main_model_endpoint;
    pub static ref MAIN_MODEL_API_KEY: &'static str = &CONFIG.model.main_model_api_key;
    pub static ref JUDGE_MODEL_ENDPOINT: &'static str = &CONFIG.model.judge_model_endpoint;
    pub static ref JUDGE_MODEL_API_KEY: &'static str = &CONFIG.model.judge_model_api_key;
    pub static ref ASK_DEVELOPER_PROMPT: &'static str = &CONFIG.prompt.ask_developer_prompt;
    pub static ref DEEP_SEARCH_DEVELOPER_PROMPT: &'static str = &CONFIG.prompt.deep_search_developer_prompt;
    pub static ref DEEP_SEARCH_GENERATE_PROMPT: &'static str = &CONFIG.prompt.deep_search_generate_prompt;
    pub static ref DISCORD_TOKEN: &'static str = &CONFIG.discord_token;
    pub static ref DOMAIN: &'static str = &CONFIG.server_domain;
}