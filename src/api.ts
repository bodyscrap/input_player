import { invoke } from "@tauri-apps/api/core";
import type { ButtonMapping, ControllerType } from "./types";

export const api = {
  // Controller operations
  async connectController(controllerType: ControllerType): Promise<string> {
    return await invoke("connect_controller", { controllerType });
  },

  async disconnectController(): Promise<string> {
    return await invoke("disconnect_controller");
  },

  async isControllerConnected(): Promise<boolean> {
    return await invoke("is_controller_connected");
  },

  // Playback operations
  async loadInputFile(path: string): Promise<number> {
    return await invoke("load_input_file", { path });
  },

  async loadInputSequence(frames: any[]): Promise<number> {
    return await invoke("load_input_sequence", { frames });
  },

  async startPlayback(): Promise<void> {
    return await invoke("start_playback");
  },

  async stopPlayback(): Promise<void> {
    return await invoke("stop_playback");
  },

  async pausePlayback(): Promise<void> {
    return await invoke("pause_playback");
  },

  async resumePlayback(): Promise<void> {
    return await invoke("resume_playback");
  },

  async setInvertHorizontal(invert: boolean): Promise<void> {
    return await invoke("set_invert_horizontal", { invert });
  },

  async setLoopPlayback(loop: boolean): Promise<void> {
    return await invoke("set_loop_playback", { loop });
  },

  async isPlaying(): Promise<boolean> {
    return await invoke("is_playing");
  },

  async getPlaybackProgress(): Promise<[number, number]> {
    return await invoke("get_playback_progress");
  },

  // Button mapping operations
  async loadButtonMapping(path: string): Promise<ButtonMapping> {
    return await invoke("load_button_mapping", { path });
  },

  async saveButtonMapping(path: string, mapping: ButtonMapping): Promise<void> {
    return await invoke("save_button_mapping", { path, mapping });
  },

  // Manual input
  async updateManualInput(
    direction: number,
    buttons: Record<string, number>,
  ): Promise<void> {
    return await invoke("update_manual_input", { direction, buttons });
  },

  // CSV button names
  async getCsvButtonNames(path: string): Promise<string[]> {
    return await invoke("get_csv_button_names", { path });
  },

  // FPS operations
  async setFps(fps: number): Promise<void> {
    return await invoke("set_fps", { fps });
  },

  async getFps(): Promise<number> {
    return await invoke("get_fps");
  },

  // Frame editor operations
  async loadFramesForEdit(path: string): Promise<any[]> {
    return await invoke("load_frames_for_edit", { path });
  },

  async saveFramesForEdit(path: string, frames: any[]): Promise<void> {
    return await invoke("save_frames_for_edit", { path, frames });
  },

  async getCurrentPlayingFrame(): Promise<number> {
    return await invoke("get_current_playing_frame");
  },

  async openEditorWindow(csvPath: string): Promise<void> {
    return await invoke("open_editor_window", { csvPath });
  },

  // 編集内容をスロットに反映（再ロード）
  async reloadCurrentSequence(): Promise<void> {
    return await invoke("reload_current_sequence");
  },
};
