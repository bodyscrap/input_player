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
pub struct UserButton {
    pub user_button: String,
    pub controller_button: Vec<String>,
    pub use_in_sequence: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonMapping {
    pub controller_type: ControllerType,
    pub mapping: Vec<UserButton>,
}

// シーケンスの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SequenceState {
    NoSequence,  // シーケンス無し
    Stopped,     // 停止状態（シーケンスはロード済み）
    Playing,     // 再生中
}

// シーケンスイベントは現在未使用のため削除（Player#get_event も削除）

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
