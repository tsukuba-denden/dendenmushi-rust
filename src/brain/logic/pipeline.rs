use crate::brain::{err::ObsError, state::{Event, Response, State}};

impl State {
    pub async fn handle_event<F>(&mut self, progress: F, event: Event, ch_id: &str, ch_name: &str) -> Result<Response, ObsError> 
    where F: Fn(Response)
    {




        Err(ObsError::IndexOutOfBounds)
    }
}