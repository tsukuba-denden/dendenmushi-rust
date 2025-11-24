use std::str::FromStr;

use openai_dive::v1::resources::response::response::Role;
use serde_json::json;
use serenity::all::{Builder, ChannelId, ChannelType, CreateMessage, CreateThread, EditMessage, GetMessages, Message, MessageId, ReactionType};

use crate::lmclient::LMTool;

pub struct DiscordToolReaction;

impl DiscordToolReaction {
    pub fn new() -> DiscordToolReaction {
        DiscordToolReaction {}
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordToolReaction {
    fn name(&self) -> String {
        "discord-tool-reaction".to_string()
    }

    fn description(&self) -> String {
        "Add or remove reactions on Discord messages.".to_string()
    }


    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform: 'add' or 'remove'.",
                    "enum": ["add", "remove"]
                },
                "channel_id": {
                    "type": "string",
                    "description": "ID of the target channel on Discord."
                },
                "message_id": {
                    "type": "string",
                    "description": "ID of the target message on Discord."
                },
                "reaction": {
                    "type": "string",
                    "description": "Emoji for reactions . Unicode (e.g. 🫠,😱,👍,👈,🤔) or custom emoji ID.",
                },
            },
            "required": ["action", "channel_id", "message_id", "reaction"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ob_ctx: crate::context::ObserverContext) -> Result<String, String> {
        let action = args.get("action")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'action' parameter".to_string())?;
        let channel_id = args.get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id' parameter".to_string())?;
        let message_id = args.get("message_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'message_id' parameter".to_string())?;
        let reaction = args.get("reaction")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'reaction' parameter".to_string())?;

        let http = ob_ctx.discord_client.open().http.clone();

        let channel_id = ChannelId::from_str(channel_id).map_err(|e| format!("Invalid 'channel_id': {}", e))?;
        let message_id = MessageId::from_str(message_id).map_err(|e| format!("Invalid 'message_id': {}", e))?;

        if action == "add" {
            channel_id.create_reaction(
                http, 
                message_id, 
                ReactionType::Unicode(reaction.into())
            ).await.map_err(|e| format!("Failed to execute Discord reaction action: {}", e))?;
        } else if action == "remove" {
            channel_id.delete_reaction_emoji(
                http, 
                message_id, 
                ReactionType::Unicode(reaction.into())
            ).await.map_err(|e| format!("Failed to execute Discord reaction action: {}", e))?;
        } else {
            return Err("Invalid 'action' parameter. Must be 'add' or 'remove'.".to_string());
        }

        // Placeholder logic for executing the Discord action
        Ok(format!("Executed action '{}' on channel ID '{}', message ID '{}', with reaction '{}'", action, channel_id, message_id, reaction))
    }
}


pub struct DiscordToolThread;

impl DiscordToolThread {
    pub fn new() -> DiscordToolThread {
        DiscordToolThread {}
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordToolThread {
    fn name(&self) -> String {
        "discord-tool-thread".to_string()
    }

    fn description(&self) -> String {
        "Create Discord threads from a message or directly under a channel."
            .to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "ID of the target channel on Discord."
                },
                "message_id": {
                    "type": "string",
                    "description": "Optional ID of the message to create the thread from. If omitted, a standalone thread is created in the channel."
                },
                "name": {
                    "type": "string",
                    "description": "Name of the thread."
                },
                "thread_type": {
                    "type": "string",
                    "description": "Type of the thread: 'public' or 'private'. Defaults to 'public'.",
                    "enum": ["public", "private"]
                },
            },
            "required": ["channel_id", "name"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        // ---- 引数パース ----
        let channel_id_str = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id' parameter".to_string())?;

        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'name' parameter".to_string())?;

        let message_id_str_opt = args
            .get("message_id")
            .and_then(|v| v.as_str());

        let thread_type_str = args
            .get("thread_type")
            .and_then(|v| v.as_str())
            .unwrap_or("public");
        // ---- ID 変換 ----
        let channel_id = ChannelId::from_str(channel_id_str)
            .map_err(|e| format!("Invalid 'channel_id': {e}"))?;

        let message_id_opt: Option<MessageId> = match message_id_str_opt {
            Some(s) => {
                let mid = MessageId::from_str(s)
                    .map_err(|e| format!("Invalid 'message_id': {e}"))?;
                Some(mid)
            }
            None => None,
        };

        // ---- ThreadType / Archive ----
        let channel_type = match thread_type_str {
            "public" => ChannelType::PublicThread,
            "private" => ChannelType::PrivateThread,
            other => {
                return Err(format!(
                    "Unsupported 'thread_type': {other}. Use 'public' or 'private'."
                ));
            }
        };

        // serenity の Http (Arc<Http>) を取得
        let http = ob_ctx.discord_client.open().http.clone();

        // ---- CreateThread builder 構築 ----
        let builder = CreateThread::new(name).kind(channel_type);

        // ---- 実行 ----
        // (channel_id, Option<message_id>) で、「メッセージから」か「スタンドアロン」かを切り替える
        let res = builder
            .execute(&http, (channel_id, message_id_opt))
            .await
            .map_err(|e| format!("Failed to create thread: {e}"))?;

        let mut context = ob_ctx.chat_contexts.get_or_create(channel_id);
        context.add_text("The context has been moved to the newly created thread. You are now inside the thread you created.".to_string(), Role::System);
        ob_ctx.chat_contexts.marge(res.id, &context);
        ob_ctx.chat_contexts.set_enabled(res.id, true);

        Ok(format!(
            "Created {thread_type_str} thread '{}' in channel_id='{}' (from message_id='{}')",
            name,
            channel_id_str,
            message_id_str_opt.unwrap_or("-")
        ))
    }
}

pub struct DiscordToolSendMessage;

impl DiscordToolSendMessage {
    pub fn new() -> DiscordToolSendMessage {
        DiscordToolSendMessage {}
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordToolSendMessage {
    fn name(&self) -> String {
        "discord-tool-send-message".to_string()
    }

    fn description(&self) -> String {
        "Send a message to a Discord channel, optionally as a reply.".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "ID of the target channel on Discord."
                },
                "content": {
                    "type": "string",
                    "description": "Message content to send."
                },
                "reply_to": {
                    "type": "string",
                    "description": "Optional message ID to reply to."
                },
            },
            "required": ["channel_id", "content"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        let channel_id_str = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id' parameter".to_string())?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'content' parameter".to_string())?;

        let reply_to_str = args
            .get("reply_to")
            .and_then(|v| v.as_str());

        let channel_id = ChannelId::from_str(channel_id_str)
            .map_err(|e| format!("Invalid 'channel_id': {e}"))?;

        let http = ob_ctx.discord_client.open().http.clone();

        // メッセージビルダー
        let mut builder = CreateMessage::new().content(content);

        if let Some(reply_id_str) = reply_to_str {
            let reply_id = MessageId::from_str(reply_id_str)
                .map_err(|e| format!("Invalid 'reply_to' message_id: {e}"))?;
            builder = builder.reference_message((channel_id, reply_id));
        }

        let msg = channel_id
            .send_message(&http, builder)
            .await
            .map_err(|e| format!("Failed to send message: {e}"))?;

        let result = json!({
            "status": "ok",
            "channel_id": channel_id_str,
            "message_id": msg.id.to_string(),
            "content": msg.content,
        });

        Ok(result.to_string())
    }
}

pub struct DiscordToolEditMessage;

impl DiscordToolEditMessage {
    pub fn new() -> DiscordToolEditMessage {
        DiscordToolEditMessage {}
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordToolEditMessage {
    fn name(&self) -> String {
        "discord-tool-edit-message".to_string()
    }

    fn description(&self) -> String {
        "Edit an existing Discord message (usually one sent by the bot).".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "ID of the channel that contains the message."
                },
                "message_id": {
                    "type": "string",
                    "description": "ID of the message to edit."
                },
                "content": {
                    "type": "string",
                    "description": "New content of the message."
                }
            },
            "required": ["channel_id", "message_id", "content"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        let channel_id_str = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id' parameter".to_string())?;

        let message_id_str = args
            .get("message_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'message_id' parameter".to_string())?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'content' parameter".to_string())?;

        let channel_id = ChannelId::from_str(channel_id_str)
            .map_err(|e| format!("Invalid 'channel_id': {e}"))?;
        let message_id = MessageId::from_str(message_id_str)
            .map_err(|e| format!("Invalid 'message_id': {e}"))?;

        let http = ob_ctx.discord_client.open().http.clone();

        let builder = EditMessage::new().content(content);

        let msg = channel_id
            .edit_message(&http, message_id, builder)
            .await
            .map_err(|e| format!("Failed to edit message: {e}"))?;

        let result = json!({
            "status": "ok",
            "channel_id": channel_id_str,
            "message_id": message_id_str,
            "content": msg.content,
        });

        Ok(result.to_string())
    }
}

pub struct DiscordToolFetchMessage;

impl DiscordToolFetchMessage {
    pub fn new() -> DiscordToolFetchMessage {
        DiscordToolFetchMessage {}
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordToolFetchMessage {
    fn name(&self) -> String {
        "discord-tool-fetch-message".to_string()
    }

    fn description(&self) -> String {
        "Fetch a single Discord message by channel_id and message_id.".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "ID of the channel that contains the message."
                },
                "message_id": {
                    "type": "string",
                    "description": "ID of the message to fetch."
                }
            },
            "required": ["channel_id", "message_id"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        let channel_id_str = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id' parameter".to_string())?;

        let message_id_str = args
            .get("message_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'message_id' parameter".to_string())?;

        let channel_id = ChannelId::from_str(channel_id_str)
            .map_err(|e| format!("Invalid 'channel_id': {e}"))?;
        let message_id = MessageId::from_str(message_id_str)
            .map_err(|e| format!("Invalid 'message_id': {e}"))?;

        let http = ob_ctx.discord_client.open().http.clone();

        let msg = channel_id
            .message(&http, message_id)
            .await
            .map_err(|e| format!("Failed to fetch message: {e}"))?;

        let result = json!({
            "status": "ok",
            "channel_id": channel_id_str,
            "message_id": message_id_str,
            "author_id": msg.author.id.to_string(),
            "author_name": msg.author.name,
            "content": msg.content,
            "timestamp": msg.timestamp.to_string(),
        });

        Ok(result.to_string())
    }
}

pub struct DiscordToolSearchMessages;

impl DiscordToolSearchMessages {
    pub fn new() -> DiscordToolSearchMessages {
        DiscordToolSearchMessages {}
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordToolSearchMessages {
    fn name(&self) -> String {
        "discord-tool-search-messages".to_string()
    }

    fn description(&self) -> String {
        "Search recent messages in a specific channel by keyword (simple local filter).".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "ID of the channel to search in."
                },
                "query": {
                    "type": "string",
                    "description": "Keyword to search for in message content."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max number of recent messages to scan (1-100). Defaults to 50."
                }
            },
            "required": ["channel_id", "query"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        let channel_id_str = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id' parameter".to_string())?;

        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'query' parameter".to_string())?;

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .min(100) as u8;

        let channel_id = ChannelId::from_str(channel_id_str)
            .map_err(|e| format!("Invalid 'channel_id': {e}"))?;

        let http = ob_ctx.discord_client.open().http.clone();

        // 直近 limit 件取得
        let messages: Vec<Message> = channel_id
            .messages(
                &http,
                GetMessages::new().limit(limit),
            )
            .await
            .map_err(|e| format!("Failed to fetch messages: {e}"))?;

        // content に query を含むものだけフィルタ
        let lower_query = query.to_lowercase();
        let matched: Vec<serde_json::Value> = messages
            .into_iter()
            .filter(|m| m.content.to_lowercase().contains(&lower_query))
            .map(|m| {
                json!({
                    "message_id": m.id.to_string(),
                    "author_id": m.author.id.to_string(),
                    "author_name": m.author.name,
                    "content": m.content,
                    "timestamp": m.timestamp.to_string(),
                })
            })
            .collect();

        let result = json!({
            "status": "ok",
            "channel_id": channel_id_str,
            "query": query,
            "matched_count": matched.len(),
            "messages": matched,
        });

        Ok(result.to_string())
    }
}

