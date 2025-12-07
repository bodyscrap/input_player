// ユーザー定義ボタンの属性
export interface UserButton {
  user_button: string; // ユーザー定義ボタン名（CSVのカラム名）
  controller_button: string[]; // 割り当て対象のコントローラ側ボタン名（配列で同時押し対応）
  use_in_sequence: boolean; // シーケンスで使用するか否か
}

export interface ButtonMapping {
  controller_type: ControllerType; // コントローラータイプ
  mapping: UserButton[]; // ボタンマッピング配列（この順番で画面表示される）
}

export interface InputFrame {
  duration: number;
  direction: number;
  buttons: Record<string, number>;
  thumb_lx?: number; // 左スティック X (-32768 to 32767)
  thumb_ly?: number; // 左スティック Y (-32768 to 32767)
  thumb_rx?: number; // 右スティック X (-32768 to 32767)
  thumb_ry?: number; // 右スティック Y (-32768 to 32767)
  left_trigger?: number; // 左トリガー (0-255)
  right_trigger?: number; // 右トリガー (0-255)
}

export type ControllerType = "xbox" | "dualshock4";

export interface PlaybackProgress {
  current: number;
  total: number;
}

// シーケンススロットのデータ構造
export interface SequenceSlot {
  path: string; // ファイルパス
  frames: InputFrame[]; // メモリに展開されたフレームデータ
  compatible: boolean; // マッピングとの互換性
}
