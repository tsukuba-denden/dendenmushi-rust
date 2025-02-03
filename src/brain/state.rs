use super::memory::collection::MemoryCollection;

pub struct State {
    pub memory: MemoryCollection,
}

pub enum Event {
    UserJoin,
    UserLeave,
    UserTx,
    RxImage,
    RxVideo,
    RxAudio,
    RxFile,
    RxMessage(String),
}

pub enum Response {
    Text(String),
    Processing,
    Thinking(String),
}

impl State {
    pub fn new() -> State {
        State {
            memory: MemoryCollection::new(),
        }
    }

    pub fn set_new_place(&mut self, place_name: &str, place_id: &str) {
        self.memory.add_place(place_name, place_id);
    }
}
