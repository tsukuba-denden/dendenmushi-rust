use dashmap::DashMap;
use serenity::all::UserId;

use crate::config::Models;

pub struct UserContexts {
    pub contexts: DashMap<UserId, UserContext>,
}

#[derive(Clone)]
pub struct UserContext {
    pub user_id: UserId,
    pub main_model: Models,
}

impl UserContext {
    pub fn new(user_id: UserId) -> UserContext {
        UserContext {
            user_id,
            main_model: Models::Gpt5Nano,
        }
    }
}

impl UserContexts {
    pub fn new() -> UserContexts {
        UserContexts {
            contexts: DashMap::new(),
        }
    }

    pub fn get_or_create(&self, user_id: UserId) -> UserContext {
        self.contexts
            .entry(user_id)
            .or_insert_with(|| UserContext::new(user_id))
            .clone()
    }

    pub fn set_model(&self, user_id: UserId, model: Models) {
        self.contexts
            .entry(user_id)
            .or_insert_with(|| UserContext::new(user_id))
            .main_model = model;
    }
}