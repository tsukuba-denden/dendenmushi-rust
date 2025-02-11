use regex::Regex;
use cron::Schedule;

use crate::brain::err::ObsError;

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

    pub fn rem_user_join(&mut self) {
        self.user_join = None;
    }

    pub fn rem_user_leave(&mut self) {
        self.user_leave = None;
    }

    pub fn rem_user_tx(&mut self) {
        self.user_tx = None;
    }

    pub fn rem_rx_url(&mut self) {
        self.rx_url = None;
    }

    pub fn rem_rx_image(&mut self) {
        self.rx_image = None;
    }

    pub fn rem_rx_video(&mut self) {
        self.rx_video = None;
    }

    pub fn rem_rx_audio(&mut self) {
        self.rx_audio = None;
    }

    pub fn rem_rx_file(&mut self) {
        self.rx_file = None;
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

    pub fn clear_re_message(&mut self) {
        self.re_message.clear();
    }

    pub fn clear_timer(&mut self) {
        self.timer.clear();
    }

    pub fn clear_all(&mut self) {
        self.user_join = None;
        self.user_leave = None;
        self.user_tx = None;
        self.rx_url = None;
        self.rx_image = None;
        self.rx_video = None;
        self.rx_audio = None;
        self.rx_file = None;
        self.re_message.clear();
        self.timer.clear();
    }
}

