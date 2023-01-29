use std::sync::Arc;

use crate::sample::Sample;

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum KeyState {
    Play,
    Done,
    Release,
}

#[derive(Debug)]
pub struct Voice {
    pub sample: Arc<Sample>,
    pub time: f64,
    pub time_delta: f64,
    pub loop_mode: sofiza::loop_mode,
    pub state: KeyState,
    pub volume: f32,
    pub release_effect: f32,
}

impl KeyState {
    pub fn is_active(&self) -> bool {
        match self {
            KeyState::Play => true,
            KeyState::Release => true,
            KeyState::Done => false,
        }
    }
}
