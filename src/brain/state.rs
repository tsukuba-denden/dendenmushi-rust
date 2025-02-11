use call_agent::chat::client::OpenAIClient;

use super::{memory::collection::{MemoryCollection, MessageEntry}, prefix::settings::model::{JUDGE_MODEL_API_KEY, JUDGE_MODEL_ENDPOINT, MAIN_MODEL_API_KEY, MAIN_MODEL_ENDPOINT}};

pub struct State {
    pub memory: MemoryCollection,
    pub main_model: OpenAIClient,
    pub judge_model: OpenAIClient,
    pub system_prompt: String,
}

impl State {
    pub async fn new() -> State {
        State {
            memory: MemoryCollection::new().await,
            main_model: OpenAIClient::new(&MAIN_MODEL_ENDPOINT, Some(&MAIN_MODEL_API_KEY)),
            judge_model: OpenAIClient::new(&JUDGE_MODEL_ENDPOINT, Some(&JUDGE_MODEL_API_KEY)),
            system_prompt: "".to_string(),
        }
    }

    pub async fn set_new_place(&mut self, place_name: &str, place_id: &str) {
        self.memory.add_place(place_name, place_id).await;
    }
}

#[derive(Clone)]
pub enum Event {
    /// メッセージ受信
    RxMessage(MessageEntry),
}

pub enum Command {
    Enable(bool),
    EntryLimit(usize),
    Schedule(bool),
    Reply(bool),
    Mention(bool),
    AutoAI(bool),
    Read(bool),
}

#[derive(Debug)]
pub enum Response {
    Text(String),
    Processing,
    Thinking(String),
}