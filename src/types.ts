export interface ButtonMapping {
  xbox: Record<string, string>;
  dualshock4: Record<string, string>;
}

export interface InputFrame {
  duration: number;
  direction: number;
  buttons: Record<string, number>;
}

export type ControllerType = 'xbox' | 'dualshock4';

export interface PlaybackProgress {
  current: number;
  total: number;
}
