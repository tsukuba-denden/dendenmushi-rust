use std::f32::consts::E;

use crate::brain::{err::ObsError, state::{Command, Event, Response, State}};

impl State {
    pub async fn handle_event<F>(&self, progress: F, event: Event, ch_id: &str, ch_name: &str) -> Result<Response, ObsError> 
    where F: Fn(Response)
    {
        //self.memory.



        Err(ObsError::UnknownError)
    }

    pub async fn handle_command<F>(&self, progress: F, command: Command, ch_id: &str, ch_name: &str) -> Result<Response, ObsError> 
    where F: Fn(Response)
    {
        //self.memory.
        Err(ObsError::UnknownError)
    }
}