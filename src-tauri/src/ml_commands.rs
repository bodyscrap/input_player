//! 入力履歴抽出のTauriコマンド

#[cfg(feature = "ml")]
use serde::Serialize;
#[cfg(feature = "ml")]
use std::path::PathBuf;

#[cfg(feature = "ml")]
use crate::video::{FrameExtractor, FrameExtractorConfig};
#[cfg(feature = "ml")]
use crate::analyzer::{InputState, InputIndicatorRegion};
#[cfg(feature = "ml")]
use crate::model::load_metadata;
#[cfg(feature = "ml")]
use crate::ml::InferenceEngine;

/// 進捗情報のペイロード
#[derive(Clone, serde::Serialize)]
pub struct ExtractionProgress {
    pub current_frame: u32,
    pub total_frames: u32,
    pub message: String,
}

/// 動画から入力履歴を抽出してCSV生成（同期処理版 + 進捗通知）
/// 
/// バックエンドスレッド内で完結するため、wgpuをSend制約なしで使用可能
/// Channelを使ってフロントエンドに進捗を通知
#[cfg(feature = "ml")]
#[tauri::command]
pub fn extract_input_history(
    video_path: String,
    model_path: String,
    output_csv_path: String,
    on_progress: tauri::ipc::Channel<ExtractionProgress>,
) -> Result<String, String> {
    // このスレッド内で推論エンジンを初期化（Sendとして渡す必要なし）
    let engine = InferenceEngine::load(&PathBuf::from(&model_path))
        .map_err(|e| format!("推論エンジンの初期化エラー: {}", e))?;
    
    use std::fs;
    
    // メタデータから領域設定を取得
    let metadata = load_metadata(&PathBuf::from(&model_path))
        .map_err(|e| format!("メタデータ読み込みエラー: {}", e))?;
    
    let button_labels = metadata.button_labels.clone();
    
    let region = InputIndicatorRegion {
        x: metadata.tile_x as u32,
        y: metadata.tile_y as u32,
        width: metadata.tile_width as u32,
        height: metadata.tile_height as u32,
        rows: 1, // 最下行のみ解析
        cols: metadata.columns_per_row as u32,
    };
    
    // 一時ディレクトリ
    let temp_dir = PathBuf::from("temp/input_extraction");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("一時ディレクトリ作成エラー: {}", e))?;
    let tile_dir = temp_dir.join("tiles");
    fs::create_dir_all(&tile_dir).ok();
    
    // CSV出力準備
    let mut csv_writer = csv::Writer::from_path(&output_csv_path)
        .map_err(|e| format!("CSV作成エラー: {}", e))?;
    
    // ヘッダー行を書き込み
    let mut header = vec!["duration".to_string(), "direction".to_string()];
    header.extend(button_labels.clone());
    csv_writer.write_record(&header)
        .map_err(|e| format!("CSVヘッダー書き込みエラー: {}", e))?;
    
    // 入力状態の履歴
    let mut previous_state: Option<InputState> = None;
    let mut duration = 0u32;
    let mut total_frames = 0u32;
    
    // フレーム抽出設定
    let frame_config = FrameExtractorConfig {
        frame_interval: 1, // 全フレーム
        output_dir: temp_dir.clone(),
        image_format: "png".to_string(),
        jpeg_quality: 95,
    };
    
    let extractor = FrameExtractor::new(frame_config);
    
    // 同期処理: フレーム抽出とタイル推論を同じスレッド内で実行
    extractor.process_frames_sync(&video_path, |frame_img, frame_num| {
        total_frames = frame_num + 1;
        
        // 30フレームごとに進捗通知
        if frame_num % 30 == 0 {
            on_progress.send(ExtractionProgress {
                current_frame: frame_num,
                total_frames: 0, // 総フレーム数は不明（動画の最後まで処理しないと分からない）
                message: format!("{}フレーム処理中...", frame_num),
            }).ok();
        }
        
        // フレームから入力インジケータ領域のタイルを抽出
        let tiles = crate::analyzer::extract_tiles_from_image(frame_img, &region)
            .map_err(|e| anyhow::anyhow!("タイル抽出エラー: {}", e))?;
        
        // 各タイルを推論
        let mut current_state = InputState::new();
        
        for (i, tile) in tiles.into_iter().enumerate() {
            let tile_path = tile_dir.join(format!("tile_{}_{}.png", frame_num, i));
            tile.save(&tile_path)
                .map_err(|e| anyhow::anyhow!("タイル保存エラー: {}", e))?;
            
            // 推論実行（engineは同じスレッド内なのでSend不要）
            let class_name = engine.classify_image(&tile_path)
                .map_err(|e| anyhow::anyhow!("推論エラー: {}", e))?;
            
            // 入力状態に反映
            crate::analyzer::update_input_state(&mut current_state, &class_name);
            
            // タイルを削除
            fs::remove_file(&tile_path).ok();
        }
        
        // 状態が変化したらCSVに書き込み
        if let Some(ref prev) = previous_state {
            if prev != &current_state {
                let line = prev.to_csv_line(duration, &button_labels);
                csv_writer.write_record(line.split(','))
                    .map_err(|e| anyhow::anyhow!("CSV書き込みエラー: {}", e))?;
                duration = 1;
            } else {
                duration += 1;
            }
        } else {
            duration = 1;
        }
        
        previous_state = Some(current_state);
        
        Ok(())
    }).map_err(|e| format!("フレーム処理エラー: {}", e))?;
    
    // 最後の状態を書き込み
    if let Some(ref state) = previous_state {
        let line = state.to_csv_line(duration, &button_labels);
        csv_writer.write_record(line.split(','))
            .map_err(|e| format!("CSV書き込みエラー: {}", e))?;
    }
    
    csv_writer.flush()
        .map_err(|e| format!("CSVフラッシュエラー: {}", e))?;
    
    // 一時ディレクトリを削除
    fs::remove_dir_all(&temp_dir).ok();
    
    // 完了通知
    on_progress.send(ExtractionProgress {
        current_frame: total_frames,
        total_frames: total_frames,
        message: format!("完了: {}フレーム処理しました", total_frames),
    }).ok();
    
    Ok(format!("入力履歴を抽出しました: {} ({}フレーム処理)", output_csv_path, total_frames))
}

/// 学習進捗データ
#[cfg(feature = "ml")]
#[derive(Debug, Clone, Serialize)]
pub struct TrainingProgress {
    pub current_epoch: usize,
    pub total_epochs: usize,
    pub train_loss: f64,
    pub train_accuracy: f64,
    pub val_loss: f64,
    pub val_accuracy: f64,
    pub message: String,
    pub log_lines: Vec<String>,
}

/// モデル学習コマンド（非同期）
#[cfg(feature = "ml")]
#[tauri::command]
pub async fn train_classification_model(
    _app_handle: tauri::AppHandle,
    data_dir: String,
    output_path: String,
    num_epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    button_labels: Vec<String>,
    use_gpu: bool,
    on_progress: tauri::ipc::Channel<TrainingProgress>,
) -> Result<String, String> {
    use crate::ml::train_model;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tokio::task;
    
    // キャンセルフラグ
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = cancel_flag.clone();
    
    // TODO: キャンセルイベントリスナーを実装
    
    // 別スレッドで学習実行
    let result = task::spawn_blocking(move || {
        use std::sync::{Arc, Mutex};
        
        // ログバッファ（スレッド間共有）
        let log_buffer = Arc::new(Mutex::new(Vec::<String>::new()));
        let log_buffer_clone = log_buffer.clone();
        let log_buffer_clone2 = log_buffer.clone();
        
        // 初期メッセージを送信
        on_progress.send(TrainingProgress {
            current_epoch: 0,
            total_epochs: num_epochs,
            train_loss: 0.0,
            train_accuracy: 0.0,
            val_loss: 0.0,
            val_accuracy: 0.0,
            message: "学習を初期化しています...".to_string(),
            log_lines: vec![],
        }).ok();
        
        // 進捗コールバック
        let progress_callback = move |epoch: usize, train_loss: f64, train_acc: f64, val_loss: f64, val_acc: f64| {
            let logs = log_buffer_clone.lock().unwrap().clone();
            on_progress.send(TrainingProgress {
                current_epoch: epoch,
                total_epochs: num_epochs,
                train_loss,
                train_accuracy: train_acc,
                val_loss,
                val_accuracy: val_acc,
                message: format!("Epoch {}/{}", epoch, num_epochs),
                log_lines: logs,
            }).ok();
        };
        
        // ログコールバック
        let log_callback = move |log_line: String| {
            log_buffer_clone2.lock().unwrap().push(log_line);
        };
        
        train_model(
            PathBuf::from(data_dir),
            PathBuf::from(output_path),
            num_epochs,
            batch_size,
            learning_rate,
            button_labels,
            use_gpu,
            cancel_flag_clone,
            progress_callback,
            log_callback,
        )
    })
    .await
    .map_err(|e| format!("学習スレッドエラー: {}", e))?;
    
    result.map_err(|e| e.to_string())
}

/// タイル分類コマンド
#[cfg(feature = "ml")]
#[tauri::command]
pub fn classify_video_tiles(
    model_path: String,
    tiles_dir: String,
    output_dir: String,
) -> Result<ClassificationResult, String> {
    use crate::ml::classify_tiles;
    
    let classified = classify_tiles(
        PathBuf::from(model_path),
        PathBuf::from(tiles_dir),
        PathBuf::from(output_dir),
    )
    .map_err(|e| e.to_string())?;
    
    let mut summary = Vec::new();
    for (class_name, paths) in classified {
        summary.push(ClassSummary {
            class_name,
            count: paths.len(),
        });
    }
    
    Ok(ClassificationResult {
        summary,
        message: "タイル分類が完了しました".to_string(),
    })
}

#[cfg(feature = "ml")]
#[derive(Debug, Serialize)]
pub struct ClassificationResult {
    pub summary: Vec<ClassSummary>,
    pub message: String,
}

#[cfg(feature = "ml")]
#[derive(Debug, Serialize)]
pub struct ClassSummary {
    pub class_name: String,
    pub count: usize,
}

// featureが無効な場合のダミー実装
#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn extract_input_history(
    _video_path: String,
    _model_path: String,
    _output_csv_path: String,
) -> Result<String, String> {
    Err("機械学習機能が有効化されていません".to_string())
}

#[cfg(not(feature = "ml"))]
#[derive(Debug, Clone, Serialize)]
pub struct TrainingProgress {
    pub current_epoch: usize,
    pub total_epochs: usize,
    pub train_loss: f64,
    pub train_accuracy: f64,
    pub val_loss: f64,
    pub val_accuracy: f64,
    pub message: String,
    pub log_lines: Vec<String>,
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub async fn train_classification_model(
    _app_handle: tauri::AppHandle,
    _data_dir: String,
    _output_path: String,
    _num_epochs: usize,
    _batch_size: usize,
    _learning_rate: f64,
    _button_labels: Vec<String>,
    _on_progress: tauri::ipc::Channel<TrainingProgress>,
) -> Result<String, String> {
    Err("機械学習機能が有効化されていません".to_string())
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn classify_video_tiles(
    _model_path: String,
    _tiles_dir: String,
    _output_dir: String,
) -> Result<String, String> {
    Err("機械学習機能が有効化されていません".to_string())
}

/// 学習データディレクトリからボタンラベルを取得
/// 
/// buttons.txtが存在する場合はそれを読み込み、
/// 存在しない場合はdir_1-9とothers以外のフォルダ名を取得
#[cfg(feature = "ml")]
#[tauri::command]
pub fn get_button_labels_from_data_dir(data_dir: String) -> Result<Vec<String>, String> {
    use std::fs;
    use std::path::Path;
    
    let data_path = Path::new(&data_dir);
    let buttons_txt_path = data_path.join("buttons.txt");
    
    // buttons.txtが存在する場合
    if buttons_txt_path.exists() {
        let content = fs::read_to_string(&buttons_txt_path)
            .map_err(|e| format!("buttons.txtの読み込みエラー: {}", e))?;
        
        let mut labels: Vec<String> = Vec::new();
        
        // 各行を処理
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            // カンマ区切りの場合は分割
            if line.contains(',') {
                for part in line.split(',') {
                    let trimmed = part.trim().to_string();
                    if !trimmed.is_empty() {
                        labels.push(trimmed);
                    }
                }
            } else {
                // カンマがない場合は行全体を1つのラベルとして扱う
                labels.push(line.to_string());
            }
        }
        
        return Ok(labels);
    }
    
    // buttons.txtが存在しない場合はフォルダ名を取得
    let mut labels = Vec::new();
    
    if let Ok(entries) = fs::read_dir(data_path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        let name_str = name.to_string();
                        // dir_1-9とothersは除外
                        if !name_str.starts_with("dir_") && name_str != "others" {
                            labels.push(name_str);
                        }
                    }
                }
            }
        }
    }
    
    labels.sort();
    Ok(labels)
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn get_button_labels_from_data_dir(_data_dir: String) -> Result<Vec<String>, String> {
    Err("機械学習機能が有効化されていません".to_string())
}
