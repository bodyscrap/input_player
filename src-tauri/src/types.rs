use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputFrame {
    pub duration: u32,
    pub direction: u8,
    pub buttons: HashMap<String, u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonMapping {
    pub xbox: HashMap<String, String>,
    pub dualshock4: HashMap<String, String>,
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
