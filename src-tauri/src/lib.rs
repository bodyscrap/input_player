mod types;
mod controller;
mod csv_loader;
mod player;
mod analysis_commands;
mod ml_commands;

// 入力解析機能のモジュール
pub mod video;
pub mod analyzer;
pub mod model;
#[cfg(feature = "ml")]
pub mod ml;

use controller::Controller;
use csv_loader::load_csv;
use player::Player;
use types::{ButtonMapping, ControllerType, InputFrame, SequenceState};

use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::collections::HashMap;
use tauri::{Emitter, State, Manager};

pub struct AppState {
    controller: Arc<Mutex<Controller>>,
    player: Arc<Mutex<Player>>,
    fps: Arc<Mutex<u32>>,
    frame_cache: Arc<Mutex<std::collections::HashMap<String, Vec<InputFrame>>>>, // パス -> フレームデータのキャッシュ
    manual_input: Arc<Mutex<InputFrame>>, // 手動入力の現在状態
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>, // イベント発行用
    button_order: Arc<Mutex<Vec<String>>>, // ボタンマッピングの順序
    is_training: Arc<Mutex<bool>>, // 学習中フラグ
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
fn get_app_dir() -> Result<String, String> {
    // 実行ファイルのディレクトリを取得
    std::env::current_exe()
        .map_err(|e| format!("実行ファイルパスの取得エラー: {}", e))?
        .parent()
        .ok_or_else(|| "親ディレクトリが見つかりません".to_string())?
        .to_str()
        .ok_or_else(|| "パスの変換エラー".to_string())
        .map(|s| s.to_string())
}

#[tauri::command]
fn load_input_sequence(frames: Vec<types::InputFrame>, state: State<AppState>) -> Result<usize, String> {
    println!("[load_input_sequence] メモリからシーケンス読み込み - {}フレーム", frames.len());

    // 総フレーム数（durationの合計）を計算
    let total_frames: u32 = frames.iter().map(|f| f.duration).sum();
    let mut player = state.player.lock().unwrap();
    player.load_frames(frames);

    println!("[load_input_sequence] 読み込み完了 - 総フレーム数: {}", total_frames);
    Ok(total_frames as usize)
}

#[tauri::command]
fn load_input_file(path: String, state: State<AppState>) -> Result<usize, String> {
    println!("[load_input_file] 開始 - パス: {}", path);

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

    // キャッシュをチェック
    let mut cache = state.frame_cache.lock().unwrap();
    let frames = if let Some(cached_frames) = cache.get(&normalized_path) {
        // キャッシュから取得
        println!("[load_input_file] キャッシュから取得 - {}フレーム", cached_frames.len());
        cached_frames.clone()
    } else {
        // CSVを読み込んでキャッシュに保存
        println!("[load_input_file] CSVから読み込み中...");
        let loaded_frames = load_csv(&csv_path)
            .map_err(|e| format!("CSV load error: {}", e))?;
        println!("[load_input_file] CSV読み込み完了 - {}フレーム", loaded_frames.len());
        cache.insert(normalized_path.clone(), loaded_frames.clone());
        loaded_frames
    };

    // 総フレーム数（durationの合計）を計算
    let total_frames: u32 = frames.iter().map(|f| f.duration).sum();
    let mut player = state.player.lock().unwrap();
    player.load_frames(frames);
    player.set_current_path(normalized_path);

    Ok(total_frames as usize)
}

#[tauri::command]
fn start_playback(state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    let frame_count = player.frames.len();
    println!("[start_playback] シーケンスモード開始 - フレーム数: {}", frame_count);
    player.start();
    println!("[start_playback] 状態: Playing (マニュアルモード無効)");
    Ok(())
}

#[tauri::command]
fn stop_playback(state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.stop();

    // 停止時はコントローラーに中立入力を送信して、物理デバイス上で入力が残らないようにする
    {
        let mut controller = state.controller.lock().unwrap();
        let neutral_frame = InputFrame {
            duration: 1,
            direction: 5,
            buttons: HashMap::new(),
            thumb_lx: 0,
            thumb_ly: 0,
            thumb_rx: 0,
            thumb_ry: 0,
            left_trigger: 0,
            right_trigger: 0,
        };

        if let Err(e) = controller.update_input(&neutral_frame, false) {
            eprintln!("警告: 停止時の中立入力送信に失敗しました: {:?}", e);
        }
    }

    // フロントエンドに即時に停止イベントを送出
    if let Some(app) = state.app_handle.lock().unwrap().as_ref() {
        let _ = app.emit("playback-state-changed", "stopped");
    }

    println!("[stop_playback] シーケンスモード停止 (マニュアルモード有効)");
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
fn reload_current_sequence(state: State<AppState>) -> Result<(), String> {
    let player = state.player.lock().unwrap();
    let current_path = player.get_current_path()
        .ok_or_else(|| "再生中のシーケンスがありません".to_string())?;
    drop(player); // unlock before reloading

    // キャッシュをクリアして再ロード
    let mut cache = state.frame_cache.lock().unwrap();
    cache.remove(&current_path);
    drop(cache);

    // 再ロード（キャッシュなしで読み込み直す）
    load_input_file(current_path, state)?;
    Ok(())
}

#[tauri::command]
fn set_loop_playback(loop_enabled: bool, state: State<AppState>) -> Result<(), String> {
    let mut player = state.player.lock().unwrap();
    player.set_loop_playback(loop_enabled);
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
    let playing = player.get_state() == SequenceState::Playing;
    let frame_count = player.frames.len();
    let current_step = player.get_current_step();
    println!("[is_playing] チェック: playing={}, frames={}, current={}", playing, frame_count, current_step);
    playing
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
        return Err(format!("ファイルが見つかりません: {:?}", mapping_path));
    }

    // ファイルが読み取り可能かチェック
    if let Err(e) = std::fs::metadata(&mapping_path) {
        return Err(format!("ファイルにアクセスできません: {:?} ({})", mapping_path, e));
    }

    let content = std::fs::read_to_string(&mapping_path)
        .map_err(|e| format!("ファイルの読み込みエラー: {} (パス: {:?})", e, mapping_path))?;

    let mapping: ButtonMapping = serde_json::from_str(&content)
        .map_err(|e| format!("JSON解析エラー: {} (パス: {:?})", e, mapping_path))?;

    // 新フォーマットからHashMapとボタン順序を取得
    let mut button_map = HashMap::new();
    let mut button_order_vec = Vec::new();
    
    for btn in &mapping.mapping {
        if !btn.controller_button.is_empty() {
            button_map.insert(btn.user_button.clone(), btn.controller_button[0].clone());
            // シーケンスで使用するボタンのみを順序リストに追加
            if btn.use_in_sequence {
                button_order_vec.push(btn.user_button.clone());
            }
        }
    }

    // Playerにボタンマッピングを設定
    let mut player = state.player.lock().unwrap();
    player.set_button_mapping(button_map);
    
    // シーケンス用ボタンの順序を保存
    let mut button_order = state.button_order.lock().unwrap();
    *button_order = button_order_vec;

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
    // 再生モード中はマニュアル入力を無視
    let player = state.player.lock().unwrap();
    let is_playing = player.get_state() == SequenceState::Playing;
    drop(player);

    if is_playing {
        return Ok(()); // 再生中は無視
    }

    let mut controller = state.controller.lock().unwrap();

    if !controller.is_connected() {
        return Err("Controller not connected".to_string());
    }

    // マニュアルモード: 手動入力の状態を更新して即座にコントローラーに送信
    let mut manual_input = state.manual_input.lock().unwrap();
    manual_input.direction = direction;
    manual_input.buttons = buttons.clone();
    
    // 即座にコントローラーに送信
    controller.update_input(&manual_input, false)
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
    drop(current_fps);
    
    // Playerにも新しいFPSを設定
    let mut player = state.player.lock().unwrap();
    player.set_fps(fps);
    
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

#[tauri::command]
fn load_frames_for_edit(path: String) -> Result<Vec<InputFrame>, String> {
    println!("========== load_frames_for_edit ==========");
    println!("Requested path: {}", path);

    let normalized_path = path.replace('\\', "/");
    println!("Normalized path: {}", normalized_path);

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

    println!("Final CSV path: {:?}", csv_path);

    if !csv_path.exists() {
        eprintln!("✗ File not found: {:?}", csv_path);
        return Err(format!("File not found: {:?}", csv_path));
    }

    println!("✓ File exists, loading CSV...");
    let result = load_csv(&csv_path)
        .map_err(|e| {
            eprintln!("✗ CSV load error: {}", e);
            format!("CSV load error: {}", e)
        });

    if let Ok(ref frames) = result {
        println!("✓ Loaded {} frames", frames.len());
    }

    result
}

#[tauri::command]
fn save_frames_for_edit(path: String, frames: Vec<InputFrame>, state: State<AppState>) -> Result<(), String> {
    use std::fs::File;
    use std::io::Write;

    println!("[save_frames_for_edit] 開始 - パス: {}, フレーム数: {}", path, frames.len());

    let normalized_path = path.replace('\\', "/");

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

    println!("[save_frames_for_edit] 保存先: {:?}", csv_path);

    let mut file = File::create(&csv_path)
        .map_err(|e| format!("ファイル作成エラー: {}", e))?;

    // ボタン名の順序をマッピング設定から取得
    let button_order = state.button_order.lock().unwrap();
    let button_names: Vec<String> = if !button_order.is_empty() {
        // マッピング設定の順序を使用
        println!("[save_frames_for_edit] マッピング順序を使用: {:?}", button_order.as_slice());
        button_order.clone()
    } else if let Some(first_frame) = frames.first() {
        // マッピングがロードされていない場合はソート（後方互換性）
        let mut names: Vec<String> = first_frame.buttons.keys().cloned().collect();
        names.sort();
        println!("[save_frames_for_edit] ソート順を使用: {:?}", names);
        names
    } else {
        Vec::new()
    };

    // ヘッダー行を書き込み
    let mut header = vec!["duration".to_string(), "direction".to_string()];
    header.extend(button_names.clone());
    writeln!(file, "{}", header.join(","))
        .map_err(|e| format!("書き込みエラー: {}", e))?;

    // フレーム数を先に取得（ムーブ前）
    let frame_count = frames.len();

    // データ行を書き込み
    for frame in frames {
        let mut values = vec![
            frame.duration.to_string(),
            frame.direction.to_string(),
        ];

        // ヘッダーと同じ順序でボタン値を出力
        for button_name in &button_names {
            values.push(frame.buttons.get(button_name).unwrap_or(&0).to_string());
        }

        writeln!(file, "{}", values.join(","))
            .map_err(|e| format!("書き込みエラー: {}", e))?;
    }

    // 保存後にキャッシュをクリア（次回読み込み時に最新のファイルを読む）
    let mut cache = state.frame_cache.lock().unwrap();
    let was_cached = cache.remove(&normalized_path).is_some();
    println!("[save_frames_for_edit] キャッシュクリア完了 - キャッシュにあった: {}", was_cached);
    println!("[save_frames_for_edit] 保存完了 - {}行を書き込み", frame_count);

    Ok(())
}

#[tauri::command]
fn get_current_playing_frame(state: State<AppState>) -> usize {
    let player = state.player.lock().unwrap();
    player.get_current_step()
}

// `open_editor_test` (test helper) removed — unused in production code

#[tauri::command]
fn open_editor_window(app: tauri::AppHandle, csv_path: String) -> Result<(), String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    println!("========== open_editor_window ==========");
    println!("CSV path: {}", csv_path);

    // ウィンドウラベルを一意にするためにパスのハッシュを使用
    let mut hasher = DefaultHasher::new();
    csv_path.hash(&mut hasher);
    let window_label = format!("editor_{}", hasher.finish());
    println!("Window label: {}", window_label);

    // 既存のウィンドウがあれば閉じる
    if let Some(window) = app.get_webview_window(&window_label) {
        println!("Closing existing window...");
        let _ = window.close();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Base64エンコードされたCSVパスをクエリパラメータとして追加
    use base64::{Engine as _, engine::general_purpose};
    let encoded_path = general_purpose::STANDARD.encode(csv_path.as_bytes());
    let url = format!("editor.html?csvPath={}", encoded_path);
    println!("Opening URL: {}", url);

    // 新しいエディタウィンドウを開く
    println!("Building window with URL: {}", url);
    let builder = tauri::WebviewWindowBuilder::new(
        &app,
        &window_label,
        tauri::WebviewUrl::App(url.clone().into())
    )
    .title(format!("Sequence Editor - {}", csv_path.split('/').last().or_else(|| csv_path.split('\\').last()).unwrap_or(&csv_path)))
    .inner_size(1200.0, 800.0)
    .resizable(true)
    .center()
    .visible(true)
    .focused(true)
    .initialization_script(r#"
        console.log('========== Tauri Initialization Script ==========');
        console.log('Location:', window.location.href);
        window.addEventListener('error', (e) => {
            console.error('Window error:', e.message, e.filename, e.lineno);
        });
        window.addEventListener('unhandledrejection', (e) => {
            console.error('Unhandled promise rejection:', e.reason);
        });
        console.log('Initialization complete');
    "#);

    let window = builder.build().map_err(|e| {
        eprintln!("✗ Failed to build editor window: {}", e);
        e.to_string()
    })?;

    println!("✓ Editor window created");
    println!("  Label: {}", window.label());
    println!("  Title: {:?}", window.title());

    // ウィンドウの状態をチェック
    if let Ok(visible) = window.is_visible() {
        println!("  Visible: {}", visible);
    }
    if let Ok(focused) = window.is_focused() {
        println!("  Focused: {}", focused);
    }

    // イベントリスナーを追加してウィンドウのロード状態を監視
    let label = window.label().to_string();
    window.on_window_event(move |event| {
        match event {
            tauri::WindowEvent::Focused(focused) => {
                println!("[{}] Window focused: {}", label, focused);
            }
            tauri::WindowEvent::Resized(size) => {
                println!("[{}] Window resized: {}x{}", label, size.width, size.height);
            }
            tauri::WindowEvent::Moved(_) => {
                println!("[{}] Window moved", label);
            }
            tauri::WindowEvent::CloseRequested { .. } => {
                println!("[{}] Window close requested", label);
            }
            tauri::WindowEvent::Destroyed => {
                println!("[{}] Window destroyed", label);
            }
            _ => {}
        }
    });

    // 少し待ってから開発者ツールを開く
    #[cfg(debug_assertions)]
    {
        let window_clone = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            println!("Opening DevTools...");
            window_clone.open_devtools();
            println!("✓ DevTools command sent");
        });
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        controller: Arc::new(Mutex::new(Controller::new())),
        player: Arc::new(Mutex::new(Player::new())),
        fps: Arc::new(Mutex::new(60)),
        frame_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        manual_input: Arc::new(Mutex::new(InputFrame {
            duration: 1,
            direction: 5,
            buttons: HashMap::new(),
            thumb_lx: 0,
            thumb_ly: 0,
            thumb_rx: 0,
            thumb_ry: 0,
            left_trigger: 0,
            right_trigger: 0,
        })),
        app_handle: Arc::new(Mutex::new(None)),
        button_order: Arc::new(Mutex::new(Vec::new())),
        is_training: Arc::new(Mutex::new(false)),
    };

    // FPS設定に基づいて更新するタスクを起動
    let controller_clone = app_state.controller.clone();
    let player_clone = app_state.player.clone();
    let fps_clone = app_state.fps.clone();
    let app_handle_clone = app_state.app_handle.clone();

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

                    // コントローラーが接続されていない場合はスキップ
                    let controller = controller_clone.lock().unwrap();
                    if !controller.is_connected() {
                        drop(controller);
                        continue;
                    }
                    drop(controller);

                    // シーケンスモード専用のループ
                    // マニュアルモードの入力は update_manual_input で即座に送信されるため、ここでは処理しない
                    let player = player_clone.lock().unwrap();
                    let state = player.get_state();
                    drop(player);

                    if state == SequenceState::Playing {
                        // シーケンス再生モード: プレイヤーの update を呼ぶ
                        let mut player = player_clone.lock().unwrap();

                        // コントローラが接続されているかチェックして、存在すれば渡す
                        let mut controller_guard = controller_clone.lock().unwrap();
                        let controller_connected = controller_guard.is_connected();

                        let update_result = if controller_connected {
                            player.update(Some(&mut *controller_guard))
                        } else {
                            // コントローラ未接続でも再生進行は行いたいので None を渡す
                            player.update(None)
                        };

                        if let Ok((_sent, state_changed)) = update_result {
                            if state_changed {
                                let new_state = player.get_state();

                                // フロントエンドにイベント送信（ログは最小限に）
                                if let Some(app) = app_handle_clone.lock().unwrap().as_ref() {
                                    let state_str = match new_state {
                                        SequenceState::Playing => "playing",
                                        SequenceState::Stopped => "stopped",
                                        SequenceState::NoSequence => "no_sequence",
                                    };
                                    let _ = app.emit("playback-state-changed", state_str);
                                    println!("[State] {:?}", new_state); // 状態変化のみ簡潔にログ
                                }
                            }
                        }
                        drop(controller_guard);
                    }
                    // マニュアルモード時はこのループでは何もしない（update_manual_inputで即座に送信）
                }
            }
        });
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(move |app| {
            // AppHandleを保存
            let handle = app.handle().clone();
            let state: tauri::State<AppState> = app.state();
            *state.app_handle.lock().unwrap() = Some(handle);
            Ok(())
        })
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            connect_controller,
            disconnect_controller,
            is_controller_connected,
            get_app_dir,
            load_input_file,
            load_input_sequence,
            start_playback,
            stop_playback,
            pause_playback,
            resume_playback,
            reload_current_sequence,
            set_loop_playback,
            set_invert_horizontal,
            is_playing,
            get_playback_progress,
            load_button_mapping,
            save_button_mapping,
            update_manual_input,
            set_fps,
            get_fps,
            get_csv_button_names,
            load_frames_for_edit,
            save_frames_for_edit,
            get_current_playing_frame,
            open_editor_window,
            // 動画解析関連のコマンド
            analysis_commands::check_gstreamer_available,
            analysis_commands::get_video_info,
            analysis_commands::save_analysis_region,
            analysis_commands::load_analysis_region,
            analysis_commands::extract_preview_frame,
            analysis_commands::extract_tiles_from_video,
            analysis_commands::collect_training_data,
            analysis_commands::create_default_classification_folders,
            analysis_commands::create_training_directory,
            analysis_commands::get_model_metadata,
            // 機械学習関連のコマンド
            ml_commands::extract_input_history,
            ml_commands::train_classification_model,
            ml_commands::classify_video_tiles,
            ml_commands::extract_and_classify_tiles,
            ml_commands::get_button_labels_from_data_dir,
            ml_commands::save_button_order_metadata,
            ml_commands::load_button_order_metadata,
            ml_commands::mp4_to_sequence,
            ml_commands::validate_mapping_and_training_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
