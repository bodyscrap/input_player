export interface ButtonMapping {
  xbox: Record<string, string>;
  dualshock4: Record<string, string>;
  sequenceButtons?: string[]; // シーケンスで使用するボタンのリスト
}

export interface InputFrame {
  duration: number;
  direction: number;
  buttons: Record<string, number>;
  thumb_lx?: number;      // 左スティック X (-32768 to 32767)
  thumb_ly?: number;      // 左スティック Y (-32768 to 32767)
  thumb_rx?: number;      // 右スティック X (-32768 to 32767)
  thumb_ry?: number;      // 右スティック Y (-32768 to 32767)
  left_trigger?: number;  // 左トリガー (0-255)
  right_trigger?: number; // 右トリガー (0-255)
}

export type ControllerType = 'xbox' | 'dualshock4';

export interface PlaybackProgress {
  current: number;
  total: number;
}
