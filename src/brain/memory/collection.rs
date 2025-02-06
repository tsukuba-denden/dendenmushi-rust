use std::{collections::HashMap, str};

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Serialize, Deserialize};

use crate::brain::{err::ObsError, prefix::settings::memory::ENTRY_LIMIT_MAX, schedule::event::CEvent};

pub struct MemoryCollection {
    pub places: HashMap<String/* pl_id */, Place>,
    pub timers: Vec<(Schedule, String/* pl_id */, String/* ch_id */)>,
}

pub struct Place {
    pub name: String,
    pub note: String,
    pub main_ch_id: String,
    pub channels: HashMap<String/* ch_id */, Channel>,
}

pub struct Channel {
    pub note: String,
    pub ch_name: String,
    pub settings: Settings,
    pub messages: Vec<MessageEntry>,
    pub schedule: CEvent,
    pub block: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageEntry {
    pub id: String,            // メッセージ ID（UUIDなど）
    pub user_id: String,       // ユーザー ID
    pub user_name: String,     // ユーザー名
    pub content: String,       // メッセージ本文
    pub timestamp: DateTime<Utc>, // 送信時間
    pub reply_to: Option<String>, // 返信先のメッセージ ID
    pub role: Role, // メッセージの種類
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Role {
    User,
    Bot,
    System,
    Assistant,
}

pub struct Settings {
    pub entry_limit: usize,
    pub schedule: bool,
    pub reply: bool,
    pub mention: bool,
    pub auto_ai: bool,
    pub read: bool,
}

impl MemoryCollection {
    pub fn new() -> Self {
        Self {
            places: HashMap::new(),
            timers: Vec::new(),
        }
    }

    pub fn add_place(&mut self, place_name: &str, place_id: &str) {
        self.places.insert(place_id.to_string(), Place::new(place_name.to_string(), place_id.to_string()));
    }
}

impl Place {
    pub fn new(name: String, main_ch_id: String) -> Self {
        let mut channels = HashMap::new();
        channels.insert(main_ch_id.to_string(), Channel::new("main".to_string(), "".to_string()));
        Self {
            name,
            main_ch_id,
            channels: HashMap::new(),
            note: "".to_string(),
        }
    }

    pub fn add_channel(&mut self, ch_name: String, ch_id: String, note: String) {
        self.channels.insert(ch_id.to_string(), Channel::new(ch_name, note));
    }
}

impl Channel {
    pub fn new(ch_name: String, note: String) -> Self {
        Self {
            ch_name,
            note,
            settings: Settings::new(),
            messages: Vec::new(),
            schedule: CEvent::new(),
            block: false,
        }
    }

    pub fn add_message(&mut self, user_id: &str, user_name: &str, content: &str, role: Role, reply_to: Option<String>, timestamp: Option<DateTime<Utc>>) {
        self.messages.push(MessageEntry {
            id: "".to_string(),
            user_id: user_id.to_string(),
            user_name: user_name.to_string(),
            content: content.to_string(),
            timestamp: timestamp.unwrap_or_else(|| Utc::now()),
            reply_to,
            role,
        });
    }
}

impl Settings {
    pub fn new() -> Self {
        Self {
            entry_limit: 4096,
            schedule: false,
            reply: true,
            mention: true,
            auto_ai: false,
            read: true,
        }
    }

    pub fn set_entry_limit(&mut self, limit: usize) -> Result<(), ObsError> {
        let limit_max = ENTRY_LIMIT_MAX;
        if limit > limit_max {
            return Err(ObsError::InvalidInput(format!("Entry limit is too large. Maximum is {}", limit_max)));
        } else {
            self.entry_limit = limit;
            Ok(())
        }
    }
}