mod types;
mod controller;
mod csv_loader;
mod player;

use controller::Controller;
use csv_loader::load_csv;
use player::Player;
use types::{ButtonMapping, ControllerType, InputFrame};

use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tauri::State;

struct AppState {
    controller: Arc<Mutex<Controller>>,
    player: Arc<Mutex<Player>>,
    fps: Arc<Mutex<u32>>,
}

// Tauri commands
#[tauri::command]
fn connect_controller(
    controller_type: String,
    state: State<AppState>,
) -> Result<String, String> {
    let ctrl_type = match controller_type.as_str() {
        "xbox" => ControllerType::Xbox,
        "dualshock4" => ControllerType::DualShock4,
        _ => return Err("Invalid controller type".to_string()),
    };

    let mut controller = state.controller.lock().unwrap();
    controller.connect(ctrl_type)
        .map_err(|e| e.to_string())?;
    
    Ok(format!("Connected to {} controller", controller_type))
}

#[tauri::command]
fn disconnect_controller(state: State<AppState>) -> Result<String, String> {
    let mut controller = state.controller.lock().unwrap();
    controller.disconnect()
        .map_err(|e| e.to_string())?;
    
    Ok("Controller disconnected".to_string())
}

#[tauri::command]
fn is_controller_connected(state: State<AppState>) -> bool {
    let controller = state.controller.lock().unwrap();
    controller.is_connected()
}

#[tauri::command]
fn load_input_file(path: String, state: State<AppState>) -> Result<usize, String> {
    // パスの区切り文字を正規化
    let normalized_path = path.replace('\\', "/");
    
    // 相対パスを絶対パスに変換
    let csv_path = if std::path::Path::new(&normalized_path).is_absolute() {
        PathBuf::from(&normalized_path)
    } else {
        let current = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let project_root = if current.ends_with("src-tauri") {
            current.parent().unwrap().to_path_buf()
        } else {
            current
        };
        project_root.join(&normalized_path)
    };
    
    if !csv_path.exists() {
        return Err(format!("File not found: {:?}", csv_path));
    }
    
    let frames = load_csv(&csv_path)
        .map_err(|e| format!("CSV load error: {}", e))?;
    
    // 総フレーム数（durationの合計）を計算
    let total_frames: u32 = frames.iter().map(|f| f.duration).sum();
    let mut player = state.player.lock().unwrap();
    player.load_frames(frames);
    
    Ok(total_frames as usize)
}

#[tauri::command]
fn start_playback(state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.start();
    Ok(())
}

#[tauri::command]
fn stop_playback(state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.stop();
    Ok(())
}

#[tauri::command]
fn pause_playback(state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.pause();
    Ok(())
}

#[tauri::command]
fn resume_playback(state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.resume();
    Ok(())
}

#[tauri::command]
fn set_invert_horizontal(invert: bool, state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.set_invert_horizontal(invert);
    Ok(())
}

#[tauri::command]
fn is_playing(state: State<AppState>) -> bool {
    let player = state.player.lock().unwrap();
    player.is_playing()
}

#[tauri::command]
fn get_playback_progress(state: State<AppState>) -> (usize, usize) {
    let player = state.player.lock().unwrap();
    player.get_progress()
}

#[tauri::command]
fn load_button_mapping(path: String, state: State<AppState>) -> Result<ButtonMapping, String> {
    // パスの区切り文字を正規化
    let normalized_path = path.replace('\\', "/");
    
    // 相対パスを絶対パスに変換
    let mapping_path = if std::path::Path::new(&normalized_path).is_absolute() {
        PathBuf::from(&normalized_path)
    } else {
        let current = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let project_root = if current.ends_with("src-tauri") {
            current.parent().unwrap().to_path_buf()
        } else {
            current
        };
        project_root.join(&normalized_path)
    };
    
    if !mapping_path.exists() {
        return Err(format!("File not found: {:?}", mapping_path));
    }
    
    let content = std::fs::read_to_string(&mapping_path)
        .map_err(|e| format!("File read error: {}", e))?;
    
    let mapping: ButtonMapping = serde_json::from_str(&content)
        .map_err(|e| format!("JSON parse error: {}", e))?;
    
    // Player\u306b\u30dc\u30bf\u30f3\u30de\u30c3\u30d4\u30f3\u30b0\u3092\u8a2d\u5b9a
    let mut player = state.player.lock().unwrap();
    player.set_button_mapping(mapping.xbox.clone());
    
    Ok(mapping)
}

#[tauri::command]
fn save_button_mapping(path: String, mapping: ButtonMapping) -> Result<(), String> {
    // パスの区切り文字を正規化
    let normalized_path = path.replace('\\', "/");
    
    // 相対パスを絶対パスに変換
    let mapping_path = if std::path::Path::new(&normalized_path).is_absolute() {
        PathBuf::from(&normalized_path)
    } else {
        let current = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let project_root = if current.ends_with("src-tauri") {
            current.parent().unwrap().to_path_buf()
        } else {
            current
        };
        project_root.join(&normalized_path)
    };
    
    // ディレクトリが存在しない場合は作成
    if let Some(parent) = mapping_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }
    
    let content = serde_json::to_string_pretty(&mapping)
        .map_err(|e| format!("JSON serialize error: {}", e))?;
    
    std::fs::write(&mapping_path, content)
        .map_err(|e| format!("File write error: {}", e))?;
    
    Ok(())
}

#[tauri::command]
fn update_manual_input(
    direction: u8,
    buttons: std::collections::HashMap<String, u8>,
    state: State<AppState>,
) -> Result<(), String> {
    let mut controller = state.controller.lock().unwrap();
    
    if !controller.is_connected() {
        return Err("Controller not connected".to_string());
    }
    
    let frame = InputFrame {
        duration: 1,
        direction,
        buttons,
        thumb_lx: 0,
        thumb_ly: 0,
        thumb_rx: 0,
        thumb_ry: 0,
        left_trigger: 0,
        right_trigger: 0,
    };
    
    controller.update_input(&frame, false)
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn set_fps(fps: u32, state: State<AppState>) -> Result<(), String> {
    if fps == 0 || fps > 240 {
        return Err("無効なFPS値です。1-240の範囲で指定してください。".to_string());
    }
    let mut current_fps = state.fps.lock().unwrap();
    *current_fps = fps;
    Ok(())
}

#[tauri::command]
fn get_fps(state: State<AppState>) -> u32 {
    let fps = state.fps.lock().unwrap();
    *fps
}

#[tauri::command]
fn get_csv_button_names(path: String) -> Result<Vec<String>, String> {
    // パスの区切り文字を正規化（\ を / に統一）
    let normalized_path = path.replace('\\', "/");
    
    // 相対パスを絶対パスに変換
    let csv_path = if std::path::Path::new(&normalized_path).is_absolute() {
        PathBuf::from(&normalized_path)
    } else {
        // 開発時は src-tauri がカレントディレクトリなので、親ディレクトリ（プロジェクトルート）からの相対パスとして解決
        let current = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let project_root = if current.ends_with("src-tauri") {
            current.parent().unwrap().to_path_buf()
        } else {
            current
        };
        project_root.join(&normalized_path)
    };
    
    if !csv_path.exists() {
        return Err(format!("File not found: {:?}", csv_path));
    }
    
    csv_loader::get_csv_button_names(&csv_path)
        .map_err(|e| format!("CSV read error: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        controller: Arc::new(Mutex::new(Controller::new())),
        player: Arc::new(Mutex::new(Player::new())),
        fps: Arc::new(Mutex::new(60)),
    };

    // FPS設定に基づいて更新するタスクを起動
    let controller_clone = app_state.controller.clone();
    let player_clone = app_state.player.clone();
    let fps_clone = app_state.fps.clone();
    
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            loop {
                // 現在のFPS設定を取得
                let current_fps = {
                    let fps = fps_clone.lock().unwrap();
                    *fps
                };
                
                let interval_ms = 1000 / current_fps;
                let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(interval_ms as u64));
                
                // FPSが変更されるまでの間ループ
                let last_fps = current_fps;
                loop {
                    interval.tick().await;
                    
                    // FPSが変更されたかチェック
                    let current_fps = {
                        let fps = fps_clone.lock().unwrap();
                        *fps
                    };
                    
                    if current_fps != last_fps {
                        // FPSが変更されたので外側ループに戻ってintervalを再生成
                        break;
                    }
                    
                    let player = player_clone.lock().unwrap();
                    
                    if player.is_playing() {
                        drop(player);
                        let mut player = player_clone.lock().unwrap();
                        let mut controller = controller_clone.lock().unwrap();
                        let _ = player.update(&mut controller);
                    }
                }
            }
        });
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            connect_controller,
            disconnect_controller,
            is_controller_connected,
            load_input_file,
            start_playback,
            stop_playback,
            pause_playback,
            resume_playback,
            set_invert_horizontal,
            is_playing,
            get_playback_progress,
            load_button_mapping,
            save_button_mapping,
            update_manual_input,
            get_csv_button_names,
            set_fps,
            get_fps,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
