use std::{collections::VecDeque, str, sync::Arc};
use call_agent::chat::{client::OpenAIClientState, function::FunctionCall, prompt::{Message, MessageContext}};
use dashmap::DashMap;

use chrono::{format, DateTime, Utc};
use cron::Schedule;
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;

use crate::brain::{err::ObsError, prefix::settings::{memory::ENTRY_LIMIT_MAX, ASSISTANT_NAME}, schedule::event::CEvent};

pub struct MemoryCollection {
    pub places: Arc<DashMap<String/* pl_id */, Arc<Place>>>,
    pub timers: Arc<Vec<(Schedule, String/* pl_id */, String/* ch_id */)>>,
}

pub struct Place {
    pub name: RwLock<String>,
    pub note: RwLock<String>,
    pub main_ch_id: RwLock<String>,
    pub channels: Arc<DashMap<String/* ch_id */, Arc<Channel>>>,
}

pub struct Channel {
    pub note: RwLock<String>,
    pub name: RwLock<String>,
    pub settings: RwLock<Settings>,
    pub messages: Arc<RwLock<VecDeque<Arc<MessageEntry>>>>,
    pub schedule: RwLock<CEvent>,
    pub using: RwLock<bool>, // このチャンネルが使用中かどうか
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageEntry {
    /// メッセージID
    pub id: String,
    /// ユーザID
    pub user_id: String,
    /// ユーザ名
    pub user_name: String,
    /// メッセージ内容
    pub content: String,
    /// メッセージのタイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// リプライ先メッセージID
    pub reply_to: Option<String>,
    pub tool_calls: Option<Vec<FunctionCall>>,
    /// ユーザの役割
    pub role: Role,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Role {
    User,
    Bot,
    System,
    Tool,
    Developer,
    Assistant,
}

pub struct Settings {
    /// チャンネルの有効/無効
    pub enable: RwLock<bool>,
    /// チャンネルの保持メッセージ数上限
    pub entry_limit: RwLock<usize>,
    /// スケジュールの有効/無効
    pub schedule: RwLock<bool>,
    /// リプライの有効/無効
    pub reply: RwLock<bool>,
    /// メンションの有効/無効
    pub mention: RwLock<bool>,
    /// 自動AIの有効/無効
    pub auto_ai: RwLock<bool>,
    /// メッセージを読むかどうか
    pub read: RwLock<bool>,
}

impl MemoryCollection {
    pub async fn new() -> Self {
        Self {
            places: Arc::new(DashMap::new()),
            timers: Arc::new(Vec::new()),
        }
    }

    pub async fn add_place(&self, place_name: &str, place_id: &str) {
        self.places.insert(place_id.to_string(), Arc::new(Place::new(place_name.to_string(), place_id.to_string(), "main_channel".to_string()).await));
    }
}

impl Place {
    pub async fn new(name: String, main_ch_id: String, main_ch_name: String) -> Self {
        let channels = DashMap::new();
        channels.insert(main_ch_id.to_string(), Arc::new(Channel::new(main_ch_name, "".to_string()).await));
        Self {
            name: RwLock::new(name),
            main_ch_id: RwLock::new(main_ch_id),
            channels: Arc::new(channels),
            note: RwLock::new("".to_string()),
        }
    }

    pub async fn add_channel(&self, ch_name: String, ch_id: String, note: String) {
        self.channels.insert(ch_id.to_string(), Arc::new(Channel::new(ch_name, note).await));
    }
}

impl Channel {
    pub async fn new(name: String, note: String) -> Self {
            let setting = Settings::new();
            let limit = *setting.entry_limit.read().await;
            Self {
                name: RwLock::new(name),
                note: RwLock::new(note),
                messages: Arc::new(RwLock::new(VecDeque::with_capacity(limit))),
                settings: RwLock::new(setting),
                schedule: RwLock::new(CEvent::new()),
                using: RwLock::new(false),
            }
        }

    pub async fn add_message(&self, message: &MessageEntry) {
        if self.messages.read().await.len() == self.messages.read().await.capacity() {
            self.messages.write().await.pop_front();
        }
        self.messages.write().await.push_back(Arc::new(message.clone()));
    }

    pub async fn latest_message(&self) -> Option<Arc<MessageEntry>> {
        let messages = self.messages.read().await;
        messages.back().map(|arc| Arc::clone(arc))
    }

    pub fn convert_message_collection_to_prompt(message: &MessageEntry) -> Message {
        match message.role {
            Role::User => {
                Message::User {
                    name: Some(message.user_name.clone()),
                    content: vec![MessageContext::Text(format!("replay_to_id: {}",message.id.clone())),MessageContext::Text(message.content.clone())],
                }
            },
            Role::Bot => {
                Message::User {
                    name: Some(format!("BOT: {}",message.user_name.clone())),
                    content: vec![MessageContext::Text(message.content.clone())],
                }
            },
            Role::System => {
                Message::System {
                    name: Some(format!("SYSTEM: {}",message.user_name.clone())),
                    content: message.content.clone(),
                }
            },
            Role::Tool => {
                Message::Tool {
                    tool_call_id: message.user_id.clone(),
                    content: vec![MessageContext::Text(message.content.clone())],
                }
            },
            Role::Developer => {
                Message::Developer {
                    name: Some(format!("DEVELOPER: {}",message.user_name.clone())),
                    content: message.content.clone(),
                }
            },
            Role::Assistant => {
                Message::Assistant {
                    name: Some(format!("YOU: {}",message.user_name.clone())),
                    content: vec![MessageContext::Text(message.content.clone())],
                    tool_calls: message.tool_calls.clone(),
                }
            },
        }
    }

    pub fn convert_prompt_to_message_collection(prompt: Message) -> MessageEntry {
        match prompt {
            Message::Assistant { name: _, content, tool_calls } => MessageEntry {
                id: String::new(),
                user_id: String::new(),
                user_name: ASSISTANT_NAME.to_string(),
                content: content.into_iter().map(|ctx| match ctx {
                    MessageContext::Text(text) => text,
                    _ => String::new(),
                }).collect::<Vec<_>>().join(" "),
                timestamp: Utc::now(),
                reply_to: None,
                tool_calls,
                role: Role::Assistant,
            },
            Message::Tool { tool_call_id, content } => MessageEntry {
                id: String::new(),
                user_id: tool_call_id,
                user_name: String::new(),
                content: content.into_iter().map(|ctx| match ctx {
                    MessageContext::Text(text) => text,
                    _ => String::new(),
                }).collect::<Vec<_>>().join(" "),
                timestamp: Utc::now(),
                reply_to: None,
                tool_calls: None,
                role: Role::Tool,
            },
            _ => {
                panic!("why assistant out user message?")
            }
        }
    }
}

impl Settings {
    pub fn new() -> Self {
            Self {
                enable: RwLock::new(false),
                entry_limit: RwLock::new(4096),
                schedule: RwLock::new(false),
                reply: RwLock::new(true),
                mention: RwLock::new(true),
                auto_ai: RwLock::new(false),
                read: RwLock::new(true),
            }
        }

    pub async fn set_entry_limit(&self, limit: usize) -> Result<(), ObsError> {
        let limit_max = *ENTRY_LIMIT_MAX;
        if limit > limit_max {
            return Err(ObsError::InvalidInput(format!("Entry limit is too large. Maximum is {}", limit_max)));
        } else {
            *self.entry_limit.write().await = limit;
            Ok(())
        }
    }
}