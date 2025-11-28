use crate::types::{ControllerType, InputFrame};
use anyhow::{Result, anyhow};
use vigem_client::{Client, TargetId, Xbox360Wired, XGamepad, XButtons};

pub struct Controller {
    target: Option<Xbox360Wired<Client>>,
    controller_type: Option<ControllerType>,
    gamepad: XGamepad,  // READMEサンプルと同様に状態を保持
}

impl Controller {
    pub fn new() -> Self {
        Self {
            target: None,
            controller_type: None,
            gamepad: XGamepad::default(),
        }
    }

    pub fn connect(&mut self, controller_type: ControllerType) -> Result<()> {
        // 既に接続されている場合は何もしない
        if self.is_connected() {
            return Ok(());
        }

        // 現在はXbox360のみサポート
        if !matches!(controller_type, ControllerType::Xbox) {
            return Err(anyhow!("DualShock4 is not yet supported"));
        }

        // ViGEmクライアントを初期化
        let client = Client::connect().map_err(|e| {
            anyhow!("Failed to connect to ViGEm: {:?}", e)
        })?;
        
        // Xbox360コントローラーを作成
        let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
        
        // プラグイン
        target.plugin().map_err(|e| {
            anyhow!("Failed to plugin Xbox controller: {:?}", e)
        })?;
        
        // 準備完了まで待機
        target.wait_ready().map_err(|e| {
            anyhow!("Failed to wait ready: {:?}", e)
        })?;

        self.target = Some(target);
        self.controller_type = Some(controller_type);
        
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        if let Some(mut target) = self.target.take() {
            target.unplug().map_err(|e| anyhow!("Failed to unplug controller: {:?}", e))?;
        }

        self.controller_type = None;

        Ok(())
    }

    pub fn update_input(&mut self, frame: &InputFrame, invert_horizontal: bool) -> Result<()> {
        // 方向入力を処理
        let (up, down, left, right) = Self::parse_direction(frame.direction, invert_horizontal);
        
        let target = self.target.as_mut().ok_or_else(|| anyhow!("Controller not connected"))?;

        // 保持しているgamepadを更新（READMEサンプルと同じパターン）
        let mut buttons_raw = 0u16;
        
        // D-Padの設定
        // XButtons定数: UP=0x0001, DOWN=0x0002, LEFT=0x0004, RIGHT=0x0008
        if up { buttons_raw |= XButtons::UP; }
        if down { buttons_raw |= XButtons::DOWN; }
        if left { buttons_raw |= XButtons::LEFT; }
        if right { buttons_raw |= XButtons::RIGHT; }

        // ボタンの設定 (button1-10にマッピング)
        // XButtons定数:
        // A=0x1000, B=0x2000, X=0x4000, Y=0x8000
        // LB=0x0100, RB=0x0200
        // LTHUMB=0x0040, RTHUMB=0x0080
        // START=0x0010, BACK=0x0020
        let mut left_trigger_value = 0u8;
        let mut right_trigger_value = 0u8;
        
        for (button_name, &value) in &frame.buttons {
            if value == 1 {
                match button_name.as_str() {
                    "button1" => buttons_raw |= XButtons::A,
                    "button2" => buttons_raw |= XButtons::B,
                    "button3" => buttons_raw |= XButtons::X,
                    "button4" => buttons_raw |= XButtons::Y,
                    "button5" => buttons_raw |= XButtons::LB,
                    "button6" => buttons_raw |= XButtons::RB,
                    "button7" => {
                        // OR結合: 既に押されている場合はそのまま
                        left_trigger_value = left_trigger_value.max(255);
                    },
                    "button8" => {
                        // OR結合: 既に押されている場合はそのまま
                        right_trigger_value = right_trigger_value.max(255);
                    },
                    "button9" => buttons_raw |= XButtons::BACK,
                    "button10" => buttons_raw |= XButtons::START,
                    "button11" => buttons_raw |= XButtons::LTHUMB,
                    "button12" => buttons_raw |= XButtons::RTHUMB,
                    _ => {}
                }
            }
        }

        // 保持しているgamepadインスタンスを更新（READMEサンプルと同様）
        self.gamepad.buttons = XButtons { raw: buttons_raw };
        self.gamepad.left_trigger = left_trigger_value;
        self.gamepad.right_trigger = right_trigger_value;
        // thumb_lx, thumb_ly, thumb_rx, thumb_ry は 0 のまま

        target.update(&self.gamepad).map_err(|e| anyhow!("Failed to update controller: {:?}", e))?;

        Ok(())
    }

    fn parse_direction(direction: u8, invert_horizontal: bool) -> (bool, bool, bool, bool) {
        let mut up = false;
        let mut down = false;
        let mut left = false;
        let mut right = false;

        match direction {
            8 => up = true,        // 上
            2 => down = true,      // 下
            4 => left = true,      // 左
            6 => right = true,     // 右
            7 => { up = true; left = true; }   // 左上
            9 => { up = true; right = true; }  // 右上
            1 => { down = true; left = true; } // 左下
            3 => { down = true; right = true; } // 右下
            _ => {} // 5 or その他: 中立
        }

        // 左右反転
        if invert_horizontal {
            std::mem::swap(&mut left, &mut right);
        }

        (up, down, left, right)
    }

    pub fn is_connected(&self) -> bool {
        self.target.is_some()
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}
