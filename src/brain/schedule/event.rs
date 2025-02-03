use regex::Regex;
use cron::Schedule;

use crate::brain::{err::ObsError, state::Event};

pub struct CEvent {
    pub user_join: Option<String /* prompt */>,
    pub user_leave: Option<String /* prompt */>,
    pub user_tx: Option<String /* prompt */>,
    pub rx_url: Option<String /* prompt */>,
    pub rx_image: Option<String /* prompt */>,
    pub rx_video: Option<String /* prompt */>,
    pub rx_audio: Option<String /* prompt */>,
    pub rx_file: Option<String /* prompt */>,
    pub re_message: Vec<(Regex/* pattern */, String/* prompt */)>,
    pub timer: Vec<(Schedule, String)>,
}

/// Interface Impl
impl CEvent {
    pub fn new() -> Self {
        Self {
            user_join: None,
            user_leave: None,
            user_tx: None,
            rx_url: None,
            rx_image: None,
            rx_video: None,
            rx_audio: None,
            rx_file: None,
            re_message: Vec::new(),
            timer: Vec::new(),
        }
    }

    pub fn set_user_join(&mut self, prompt: String) {
        self.user_join = Some(prompt);
    }

    pub fn set_user_leave(&mut self, prompt: String) {
        self.user_leave = Some(prompt);
    }

    pub fn set_user_tx(&mut self, prompt: String) {
        self.user_tx = Some(prompt);
    }

    pub fn set_rx_url(&mut self, prompt: String) {
        self.rx_url = Some(prompt);
    }

    pub fn set_rx_image(&mut self, prompt: String) {
        self.rx_image = Some(prompt);
    }

    pub fn set_rx_video(&mut self, prompt: String) {
        self.rx_video = Some(prompt);
    }

    pub fn set_rx_audio(&mut self, prompt: String) {
        self.rx_audio = Some(prompt);
    }

    pub fn set_rx_file(&mut self, prompt: String) {
        self.rx_file = Some(prompt);
    }

    pub fn add_re_message(&mut self, pattern: Regex, prompt: String) {
        self.re_message.push((pattern, prompt));
    }

    pub fn add_timer(&mut self, time: Schedule, prompt: String) -> usize{
        self.timer.push((time, prompt));
        self.timer.len() - 1    // return index
    }
    
    pub fn get_user_join(&self) -> Option<&String> {
        self.user_join.as_ref()
    }

    pub fn get_user_leave(&self) -> Option<&String> {
        self.user_leave.as_ref()
    }

    pub fn get_user_tx(&self) -> Option<&String> {
        self.user_tx.as_ref()
    }

    pub fn get_rx_url(&self) -> Option<&String> {
        self.rx_url.as_ref()
    }

    pub fn get_rx_image(&self) -> Option<&String> {
        self.rx_image.as_ref()
    }

    pub fn get_rx_video(&self) -> Option<&String> {
        self.rx_video.as_ref()
    }

    pub fn get_rx_audio(&self) -> Option<&String> {
        self.rx_audio.as_ref()
    }

    pub fn get_rx_file(&self) -> Option<&String> {
        self.rx_file.as_ref()
    }

    pub fn get_re_message_list(&self) -> &Vec<(Regex, String)> {
        &self.re_message
    }

    pub fn get_timer(&self) -> &Vec<(Schedule, String)> {
        &self.timer
    }

    pub fn rem_re_message(&mut self, index: usize) -> Result<(), ObsError> {
        if index < self.re_message.len() {
            self.re_message.remove(index);
            Ok(())
        } else {
            Err(ObsError::IndexOutOfBounds)
        }
    }

    pub fn rem_timer(&mut self, index: usize) -> Result<(), ObsError> {
        if index < self.timer.len() {
            self.timer.remove(index);
            Ok(())
        } else {
            Err(ObsError::IndexOutOfBounds)
        }
    }
}

/// Method Impl
impl CEvent {
    pub fn check_msg_event(&self, message: Event) -> Option<&String> {
        match message {
            Event::UserJoin => self.get_user_join(),
            Event::UserLeave => self.get_user_leave(),
            Event::UserTx => self.get_user_tx(),
            Event::RxImage => self.get_rx_image(),
            Event::RxVideo => self.get_rx_video(),
            Event::RxAudio => self.get_rx_audio(),
            Event::RxFile => self.get_rx_file(),
            Event::RxMessage(msg) => {
                if let Some(prompt) = self.match_rx_url(&msg) {
                    return Some(prompt);
                } else {
                    return self.match_rx_message(&msg);
                }
            },
        }
    }

    pub fn match_rx_url(&self, message: &str) -> Option<&String> {
        if let Some(prompt) = self.rx_url.as_ref() {
            let rex = Regex::new(r".*?(https?://[\w/:%#\$&\?\(\)~\.=\+\-]+).*?").unwrap();
            if rex.is_match(message) {
                return Some(prompt);
            }
        }
        None
    }

    pub fn match_rx_message(&self, message: &str) -> Option<&String> {
        for (pattern, prompt) in &self.re_message {
            if pattern.is_match(message) {
                return Some(prompt);
            }
        }
        None
    }
}