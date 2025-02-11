use std::os::windows::io::AsRawHandle;

use call_agent::chat::prompt::{Message, MessageContext};

use super::{err::ObsError, memory::collection::Channel, prefix::settings::MAX_USE_TOOL_COUNT, state::{Command, Event, Response, State}};

impl State {
    pub async fn set_system_prompt(&mut self, prompt: &str) {
        self.system_prompt = prompt.to_string();
    }

    pub async fn add_event<F>(&self, progress: F, event: Event, pl_id: &str,ch_id: &str) -> Result<(), ObsError> 
    where F: Fn(Response)
    {
        let pl = self.memory.places.get(pl_id).ok_or(ObsError::NotFoundPlace)?;
        let ch = pl.channels.get(ch_id).ok_or(ObsError::NotFoundChannel)?;

        // チャンネルが無効化されている場合はエラーを返す
        let settings = &ch.settings;
        if !*settings.read().await.enable.read().await {
            return Err(ObsError::DisabledChannel);
        }
        Err(ObsError::UnknownError)
    }

    /// 一般のメッセージハンドルです。
    pub async fn handle_event<F>(&self, _progress: F, event: Event, pl_id: &str, ch_id: &str) -> Result<Response, ObsError> 
    where F: Fn(Response)
    {
        // プレイスとチャンネルを取得
        let pl = self.memory.places.get(pl_id).ok_or(ObsError::NotFoundPlace)?;
        let ch = pl.channels.get(ch_id).ok_or(ObsError::NotFoundChannel)?;

        // チャンネルが無効化されている場合はエラーを返す
        let settings = &ch.settings;
        if !*settings.read().await.enable.read().await {
            return Err(ObsError::DisabledChannel);
        }

        match event.clone() {
            Event::RxMessage(message) => {
                // メッセージを追加
                ch.add_message(&message).await;
            },
        }

        // システムプロンプトを生成
        let system_prompt = format!("
            now place: '{}'-'{}';
            now channel: '{}'-'{}';
            {};
            ",
            pl.name.read().await, 
            pl.note.read().await, 
            ch.name.read().await, 
            ch.note.read().await, 
            self.system_prompt
        );

        // プロンプトを生成
        let mut prompt = self.main_model.create_prompt();
        for message in ch.messages.read().await.iter() {
            prompt.add(
                vec![Channel::convert_message_collection_to_prompt(message)]
            ).await;
        }

        // イベントによって処理を分岐
        match event {
            Event::RxMessage(message) => {
                prompt.add(
                    vec![Channel::convert_message_collection_to_prompt(&message)]
                ).await;
            },
        }

        prompt.add(
            vec![Message::System { name: None, content: system_prompt }]
        ).await;

        // プロンプトから応答を生成
        for i in 0..MAX_USE_TOOL_COUNT {
            if i == 5 {
            let _ = prompt.generate(None).await;
            match prompt.last().await.ok_or_else(|| {
                println!("Unknown error occurred: prompt.last() returned None");
                ObsError::UnknownError
            })? {
                Message::Assistant { name: _, content, tool_calls: _ } => {
                return Ok(Response::Text(
                    match content.first().ok_or_else(|| {
                    println!("Unknown error occurred: content.first() returned None");
                    ObsError::UnknownError
                    })? {
                    MessageContext::Text(text) => text.clone(),
                    _ => {
                        println!("Unknown error occurred: MessageContext is not Text");
                        return Err(ObsError::UnknownError)
                    },
                    }
                ));
                }
                _ => {
                println!("Unknown error occurred: Unexpected message variant");
                return Err(ObsError::UnknownError)
                },
            }
            } else {
            let _ = prompt.generate_can_use_tool(None).await;
            match prompt.last().await.ok_or_else(|| {
                println!("Unknown error occurred: prompt.last() returned None (can_use_tool)");
                ObsError::UnknownError
            })? {
                Message::Tool { tool_call_id: _, content: _ } => {
                let _ = prompt.generate_can_use_tool(None).await;
                },
                Message::Assistant { name: _, content, tool_calls: _ } => {
                return Ok(Response::Text(
                    match content.first().ok_or_else(|| {
                    println!("Unknown error occurred: content.first() returned None (assistant, can_use_tool)");
                    ObsError::UnknownError
                    })? {
                    MessageContext::Text(text) => text.clone(),
                    _ => {
                        println!("Unknown error occurred: MessageContext is not Text (assistant, can_use_tool)");
                        return Err(ObsError::UnknownError)
                    },
                    }
                ));
                }
                _ => {
                println!("Unknown error occurred: Unexpected message variant (can_use_tool)");
                return Err(ObsError::UnknownError)
                },
            }
            }
        }


        Err(ObsError::UnknownError)
    }


    /// コマンドハンドルです。
    pub async fn handle_command<F>(&mut self, _progress: F, command: Command, pl_id: &str, ch_id: &str) -> Result<Response, ObsError> 
    where F: Fn(Response)
    {
        let pl = self.memory.places.get_mut(pl_id).ok_or(ObsError::NotFoundPlace)?;
        let ch = pl.channels.get_mut(ch_id).ok_or(ObsError::NotFoundChannel)?;

        match command {
            Command::Enable(enable) => {
                *ch.settings.write().await.enable.write().await = enable;
                return Ok(Response::Text(format!("Channel {} is now {}", ch.name.read().await, if enable { "enabled" } else { "disabled" })));
            },
            Command::EntryLimit(limit) => {
                match ch.settings.write().await.set_entry_limit(limit).await {
                    Ok(_) => {
                        return Ok(Response::Text(format!("Entry limit of channel {} is now set to {}", ch.name.read().await, limit)));
                    },
                    Err(e) => {
                        return Ok(Response::Text(format!("Failed to set entry limit of channel {}: {}", ch.name.read().await, e)));
                    },
                }
            },
            Command::Schedule(schedule) => {
                *ch.settings.write().await.schedule.write().await = schedule;
                return Ok(Response::Text(format!("Schedule of channel {} is now {}", ch.name.read().await, if schedule { "enabled" } else { "disabled" })));
            },
            Command::Reply(reply) => {
                *ch.settings.write().await.reply.write().await = reply;
                return Ok(Response::Text(format!("Reply of channel {} is now {}", ch.name.read().await, if reply { "enabled" } else { "disabled" })));
            },
            Command::Mention(mention) => {
                *ch.settings.write().await.mention.write().await = mention;
                return Ok(Response::Text(format!("Mention of channel {} is now {}", ch.name.read().await, if mention { "enabled" } else { "disabled" })));
            },
            Command::AutoAI(auto_ai) => {
                *ch.settings.write().await.auto_ai.write().await = auto_ai;
                return Ok(Response::Text(format!("AutoAI of channel {} is now {}", ch.name.read().await, if auto_ai { "enabled" } else { "disabled" })));
            },
            Command::Read(read) => {
                *ch.settings.write().await.read.write().await = read;
                return Ok(Response::Text(format!("Read of channel {} is now {}", ch.name.read().await, if read { "enabled" } else { "disabled" })));
            },
        }
    }
}