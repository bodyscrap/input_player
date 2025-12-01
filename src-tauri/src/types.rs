use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputFrame {
    pub duration: u32,
    pub direction: u8,
    pub buttons: HashMap<String, u8>,
    
    // アナログ軸 (オプション。CSVからの再生時は使用しない)
    #[serde(default)]
    pub thumb_lx: i16,  // 左スティック X (-32768 to 32767)
    #[serde(default)]
    pub thumb_ly: i16,  // 左スティック Y (-32768 to 32767)
    #[serde(default)]
    pub thumb_rx: i16,  // 右スティック X (-32768 to 32767)
    #[serde(default)]
    pub thumb_ry: i16,  // 右スティック Y (-32768 to 32767)
    #[serde(default)]
    pub left_trigger: u8,  // 左トリガー (0-255)
    #[serde(default)]
    pub right_trigger: u8, // 右トリガー (0-255)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonMapping {
    pub xbox: HashMap<String, String>,
    pub dualshock4: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "sequenceButtons")]
    pub sequence_buttons: Option<Vec<String>>,
}

// シーケンスの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SequenceState {
    NoSequence,  // シーケンス無し
    Stopped,     // 停止状態（シーケンスはロード済み）
    Playing,     // 再生中
}

// シーケンスイベント（フロントエンドへの通知用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceEvent {
    pub state: SequenceState,
    pub current_step: usize,  // 現在のステップ（行番号）
    pub total_steps: usize,   // 総ステップ数
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControllerType {
    Xbox,
    DualShock4,
}

impl std::fmt::Display for ControllerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControllerType::Xbox => write!(f, "Xbox"),
            ControllerType::DualShock4 => write!(f, "DualShock4"),
        }
    }
}
