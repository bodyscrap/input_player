use crate::controller::Controller;
use crate::types::{InputFrame, SequenceState, SequenceEvent};
use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct Player {
    // シーケンスデータ
    pub frames: Vec<InputFrame>,
    
    // 状態管理
    state: SequenceState,
    current_step: usize,  // 現在のステップ（行番号）
    
    // タイミング管理
    sequence_start_time: Option<Instant>,  // シーケンス開始時刻
    next_step_time: Duration,  // 次のステップに進む累積時間
    
    // 設定
    invert_horizontal: bool,
    button_mapping: HashMap<String, String>, // CSVボタン名 -> Xboxボタン名
    loop_playback: bool,
    current_path: Option<String>,
    fps: u32,  // FPS設定
}

impl Player {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            state: SequenceState::NoSequence,
            current_step: 0,
            sequence_start_time: None,
            next_step_time: Duration::from_secs(0),
            invert_horizontal: false,
            button_mapping: HashMap::new(),
            loop_playback: false,
            current_path: None,
            fps: 60,
        }
    }

    // シーケンスをロード（停止状態に遷移）
    pub fn load_frames(&mut self, frames: Vec<InputFrame>) {
        self.frames = frames;
        self.state = if self.frames.is_empty() {
            SequenceState::NoSequence
        } else {
            SequenceState::Stopped
        };
        self.current_step = 0;
        self.sequence_start_time = None;
        self.next_step_time = Duration::from_secs(0);
    }

    // 再生開始
    pub fn start(&mut self) {
        if self.state == SequenceState::Stopped || self.state == SequenceState::NoSequence {
            if !self.frames.is_empty() {
                // 1. シーケンスロード完了 (既に完了)
                // 2. コントローラー状態を入力無しに (次のupdateで送信)
                // 3. 再生開始時刻を取得
                self.sequence_start_time = Some(Instant::now());
                // 4. 再生中に遷移
                self.state = SequenceState::Playing;
                // 5. コントローラの状態を最初のステップに設定 (次のupdateで送信)
                self.current_step = 0;
                // next_step_time は開始時刻からの絶対経過時間 (0ms = すぐに送信)
                self.next_step_time = Duration::from_secs(0);
                
                println!("[Player] 再生開始: {} steps", self.frames.len());
            }
        }
    }

    // 停止
    pub fn stop(&mut self) {
        if self.state == SequenceState::Playing {
            self.state = SequenceState::Stopped;
            self.current_step = 0;
            self.sequence_start_time = None;
            self.next_step_time = Duration::from_secs(0);
            println!("[Player] 停止");
        }
    }

    pub fn pause(&mut self) {
        self.stop();
    }

    pub fn resume(&mut self) {
        self.start();
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

    pub fn set_fps(&mut self, fps: u32) {
        self.fps = fps;
    }

    pub fn is_playing(&self) -> bool {
        self.state == SequenceState::Playing
    }

    pub fn get_state(&self) -> SequenceState {
        self.state
    }

    pub fn get_event(&self) -> SequenceEvent {
        SequenceEvent {
            state: self.state,
            current_step: self.current_step,
            total_steps: self.frames.len(),
        }
    }

    // メインループから呼ばれる更新関数
    // controller_opt が Some の場合はコントローラーへ入力を送信する。
    // None の場合はコントローラー送信をスキップするが、再生進行自体は行う。
    // 戻り値: (コントローラーに送信したか, 状態が変化したか)
    pub fn update(&mut self, controller_opt: Option<&mut Controller>) -> Result<(bool, bool)> {
        if self.state != SequenceState::Playing || self.frames.is_empty() {
            return Ok((false, false));
        }

        let start_time = match self.sequence_start_time {
            Some(t) => t,
            None => return Ok((false, false)),
        };

        // 8. 再生開始時間からの経過時間を取得
        let elapsed = start_time.elapsed();
        let mut state_changed = false;

        // 9. 開始時刻からの絶対経過時間で送信時刻を管理 (累積誤差を防ぐ)
        // 10. 現在時刻から次の送信時刻までの差分sleep (メインループが60FPSで呼ぶのでここではチェックのみ)
            if elapsed >= self.next_step_time {
            // 5. コントローラの状態を現在のステップの入力状態に更新
            if self.current_step < self.frames.len() {
                let frame = &self.frames[self.current_step];
                
                // ボタンマッピングを適用
                let mut mapped_frame = frame.clone();
                let mut mapped_buttons = HashMap::new();

                for (csv_button, value) in &frame.buttons {
                    if let Some(xbox_button) = self.button_mapping.get(csv_button) {
                        let current_value = mapped_buttons.get(xbox_button).unwrap_or(&0);
                        let new_value = if *current_value == 1 || *value == 1 { 1 } else { 0 };
                        mapped_buttons.insert(xbox_button.clone(), new_value);
                    }
                }
                mapped_frame.buttons = mapped_buttons;

                // 6. コントローラの状態をドライバに送信
                // コントローラーが渡されている場合のみ送信を行う
                let mut sent = false;
                if let Some(ctrl) = controller_opt {
                    if ctrl.is_connected() {
                        if ctrl.update_input(&mapped_frame, self.invert_horizontal).is_ok() {
                            sent = true;
                        }
                    }
                }

                // 次のステップの送信時刻を開始時刻からの絶対時間で計算（現在のステップをインクリメントする前）
                // 例: step0(3F) 送信後 → next_step_time = 0 + 3*1000/60 = 50ms
                //     step1(5F) 送信後 → next_step_time = 0 + (3+5)*1000/60 = 133ms
                //     step2(4F) 送信後 → next_step_time = 0 + (3+5+4)*1000/60 = 200ms
                // これにより各ステップの誤差が累積しない
                let mut cumulative_duration = 0u32;
                for i in 0..=self.current_step {
                    if i < self.frames.len() {
                        cumulative_duration += self.frames[i].duration;
                    }
                }
                let cumulative_ms = cumulative_duration as f64 * 1000.0 / self.fps as f64;
                self.next_step_time = Duration::from_millis(cumulative_ms as u64);

                // 7. コントローラの内部状態を次のステップの状態に更新
                self.current_step += 1;

                return Ok((sent, state_changed));
            } else if self.current_step >= self.frames.len() {
                // 全てのステップを送信済みで、最後のステップのdurationも経過した
                if self.loop_playback {
                    // ループ再生: 先頭に戻る
                    self.current_step = 0;
                    // ループの先頭に戻るたびに開始時刻を更新（各ループサイクルが独立した正確なタイミングで再生）
                    self.sequence_start_time = Some(Instant::now());
                    self.next_step_time = Duration::from_secs(0);
                    state_changed = true;
                    println!("[Player] ループ再生: 先頭に戻ります");
                    return Ok((false, state_changed));
                } else {
                    // 通常再生: 無入力を送信してから停止
                    let neutral_frame = InputFrame {
                        duration: 1,
                        direction: 5, // 中立
                        buttons: HashMap::new(), // 全ボタンOFF
                        thumb_lx: 0,
                        thumb_ly: 0,
                        thumb_rx: 0,
                        thumb_ry: 0,
                        left_trigger: 0,
                        right_trigger: 0,
                    };
                    // コントローラがあれば中立入力を送信する
                    let mut sent = false;
                    if let Some(ctrl) = controller_opt {
                        if ctrl.is_connected() {
                            if ctrl.update_input(&neutral_frame, false).is_ok() {
                                sent = true;
                            }
                        }
                    }

                    self.state = SequenceState::Stopped;
                    self.current_step = 0;
                    self.sequence_start_time = None;
                    self.next_step_time = Duration::from_secs(0);
                    state_changed = true;
                    println!("[Player] 再生完了: 無入力送信後、停止状態に遷移");
                    return Ok((sent, state_changed));
                }
            }
        }

        Ok((false, state_changed))
    }

    pub fn get_progress(&self) -> (usize, usize) {
        (self.current_step, self.frames.len())
    }

    pub fn get_current_step(&self) -> usize {
        self.current_step
    }

    pub fn set_current_path(&mut self, path: String) {
        self.current_path = Some(path);
    }

    pub fn get_current_path(&self) -> Option<String> {
        self.current_path.clone()
    }
}
