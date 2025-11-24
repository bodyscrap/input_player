use crate::controller::Controller;
use crate::types::InputFrame;
use anyhow::Result;

pub struct Player {
    frames: Vec<InputFrame>,
    current_frame: usize,
    current_frame_count: u32,
    is_playing: bool,
    invert_horizontal: bool,
}

impl Player {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            current_frame: 0,
            current_frame_count: 0,
            is_playing: false,
            invert_horizontal: false,
        }
    }

    pub fn load_frames(&mut self, frames: Vec<InputFrame>) {
        self.frames = frames;
        self.current_frame = 0;
        self.current_frame_count = 0;
    }

    pub fn start(&mut self) {
        self.is_playing = true;
        self.current_frame = 0;
        self.current_frame_count = 0;
    }

    pub fn stop(&mut self) {
        self.is_playing = false;
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn resume(&mut self) {
        self.is_playing = true;
    }

    pub fn set_invert_horizontal(&mut self, invert: bool) {
        self.invert_horizontal = invert;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub fn update(&mut self, controller: &mut Controller) -> Result<bool> {
        if !self.is_playing || self.frames.is_empty() {
            return Ok(false);
        }

        if self.current_frame >= self.frames.len() {
            self.is_playing = false;
            return Ok(false);
        }

        let frame = &self.frames[self.current_frame];
        
        // コントローラーに入力を送信
        controller.update_input(frame, self.invert_horizontal)?;

        self.current_frame_count += 1;

        // durationフレーム経過したら次のフレームへ
        if self.current_frame_count >= frame.duration {
            self.current_frame += 1;
            self.current_frame_count = 0;
        }

        Ok(true)
    }

    pub fn get_progress(&self) -> (usize, usize) {
        (self.current_frame, self.frames.len())
    }
}
