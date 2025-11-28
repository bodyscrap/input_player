use crate::controller::Controller;
use crate::types::InputFrame;
use anyhow::Result;
use std::collections::HashMap;

pub struct Player {
    pub frames: Vec<InputFrame>,
    current_frame: usize,
    current_frame_count: u32,
    is_playing: bool,
    invert_horizontal: bool,
    button_mapping: HashMap<String, String>, // CSVボタン名 -> Xboxボタン名
    loop_playback: bool,
    current_path: Option<String>, // 現在ロードされているCSVのパス
}

impl Player {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            current_frame: 0,
            current_frame_count: 0,
            is_playing: false,
            invert_horizontal: false,
            button_mapping: HashMap::new(),
            loop_playback: false,
            current_path: None,
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

    pub fn set_button_mapping(&mut self, mapping: HashMap<String, String>) {
        self.button_mapping = mapping;
    }

    pub fn set_loop_playback(&mut self, loop_enabled: bool) {
        self.loop_playback = loop_enabled;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub fn update(&mut self, controller: &mut Controller) -> Result<bool> {
        if !self.is_playing || self.frames.is_empty() {
            return Ok(false);
        }

        if self.current_frame >= self.frames.len() {
            if self.loop_playback {
                // ループ再生の場合は最初から再生
                self.current_frame = 0;
                self.current_frame_count = 0;
            } else {
                self.is_playing = false;
                return Ok(false);
            }
        }

        let frame = &self.frames[self.current_frame];
        
        // ボタンマッピングを適用（複数のCSVボタンが同じXboxボタンに割り当てられている場合はOR結合）
        let mut mapped_frame = frame.clone();
        let mut mapped_buttons = HashMap::new();
        
        for (csv_button, value) in &frame.buttons {
            if let Some(xbox_button) = self.button_mapping.get(csv_button) {
                // 既存の値とOR結合（どちらかが1なら1）
                let current_value = mapped_buttons.get(xbox_button).unwrap_or(&0);
                let new_value = if *current_value == 1 || *value == 1 { 1 } else { 0 };
                mapped_buttons.insert(xbox_button.clone(), new_value);
            }
        }
        mapped_frame.buttons = mapped_buttons;
        
        // コントローラーに入力を送信（エラーは無視して再生は継続）
        let _ = controller.update_input(&mapped_frame, self.invert_horizontal);

        self.current_frame_count += 1;

        // durationフレーム経過したら次のフレームへ
        if self.current_frame_count >= frame.duration {
            self.current_frame += 1;
            self.current_frame_count = 0;
        }

        Ok(true)
    }

    pub fn get_progress(&self) -> (usize, usize) {
        // 現在までの経過フレーム数を計算
        let mut elapsed_frames = 0u32;
        for i in 0..self.current_frame {
            if i < self.frames.len() {
                elapsed_frames += self.frames[i].duration;
            }
        }
        elapsed_frames += self.current_frame_count;
        
        // 総フレーム数を計算（全durationの合計）
        let total_frames: u32 = self.frames.iter().map(|f| f.duration).sum();
        
        (elapsed_frames as usize, total_frames as usize)
    }

    pub fn get_current_frame(&self) -> usize {
        self.current_frame
    }

    pub fn set_current_path(&mut self, path: String) {
        self.current_path = Some(path);
    }

    pub fn get_current_path(&self) -> Option<String> {
        self.current_path.clone()
    }
}
