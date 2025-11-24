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
    let frames = load_csv(&PathBuf::from(path))
        .map_err(|e| e.to_string())?;
    
    let frame_count = frames.len();
    let mut player = state.player.lock().unwrap();
    player.load_frames(frames);
    
    Ok(frame_count)
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
fn load_button_mapping(path: String) -> Result<ButtonMapping, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())?;
    
    let mapping: ButtonMapping = serde_json::from_str(&content)
        .map_err(|e| e.to_string())?;
    
    Ok(mapping)
}

#[tauri::command]
fn save_button_mapping(path: String, mapping: ButtonMapping) -> Result<(), String> {
    let content = serde_json::to_string_pretty(&mapping)
        .map_err(|e| e.to_string())?;
    
    std::fs::write(path, content)
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn update_manual_input(
    direction: u8,
    buttons: std::collections::HashMap<String, u8>,
    state: State<AppState>,
) -> Result<(), String> {
    let mut controller = state.controller.lock().unwrap();
    
    let frame = InputFrame {
        duration: 1,
        direction,
        buttons,
    };
    
    controller.update_input(&frame, false)
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        controller: Arc::new(Mutex::new(Controller::new())),
        player: Arc::new(Mutex::new(Player::new())),
    };

    // 60FPSで更新するタスクを起動
    let controller_clone = app_state.controller.clone();
    let player_clone = app_state.player.clone();
    
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(16)); // 約60fps
            
            loop {
                interval.tick().await;
                
                let player = player_clone.lock().unwrap();
                
                if player.is_playing() {
                    drop(player);
                    let mut player = player_clone.lock().unwrap();
                    let mut controller = controller_clone.lock().unwrap();
                    let _ = player.update(&mut controller);
                }
            }
        });
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
