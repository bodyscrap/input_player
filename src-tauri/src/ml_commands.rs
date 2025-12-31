//! 入力履歴抽出のTauriコマンド

#[cfg(feature = "ml")]
use serde::Serialize;
#[cfg(feature = "ml")]
use std::path::PathBuf;
#[cfg(feature = "ml")]
use tauri::Manager;

#[cfg(feature = "ml")]
use crate::video::{FrameExtractor, FrameExtractorConfig};
#[cfg(feature = "ml")]
use crate::analyzer::{InputState, InputIndicatorRegion};
#[cfg(feature = "ml")]
use crate::model::load_metadata;
#[cfg(feature = "ml")]
use std::fs;
#[cfg(feature = "ml")]
use crate::ml::InferenceEngine;

/// 非圧縮PNGとして画像を保存するヘルパー関数
#[cfg(feature = "ml")]
fn save_as_uncompressed_png<P: AsRef<std::path::Path>>(
    img: &image::DynamicImage,
    path: P,
) -> Result<(), anyhow::Error> {
    use image::codecs::png::{PngEncoder, CompressionType, FilterType};
    use image::ImageEncoder;
    use std::fs::File;
    use std::io::BufWriter;

    let file = File::create(path)?;
    let buf_writer = BufWriter::new(file);
    
    let encoder = PngEncoder::new_with_quality(
        buf_writer,
        CompressionType::Fast,  // 最小圧縮（実質無圧縮に近い）
        FilterType::NoFilter,   // フィルタなし
    );
    
    encoder.write_image(
        img.as_bytes(),
        img.width(),
        img.height(),
        img.color().into(),
    )?;
    
    Ok(())
}

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
    use_gpu: bool,
    on_progress: tauri::ipc::Channel<ExtractionProgress>,
) -> Result<String, String> {
    // このスレッド内で推論エンジンを初期化（Sendとして渡す必要なし）
    let engine = InferenceEngine::load_with_backend(&PathBuf::from(&model_path), use_gpu)
        .map_err(|e| format!("推論エンジンの初期化エラー: {}", e))?;
    
    // メタデータから領域設定を取得
    let metadata = load_metadata(&PathBuf::from(&model_path))
        .map_err(|e| format!("メタデータ読み込みエラー: {}", e))?;
    
    let button_labels = metadata.button_labels.clone();
    
    // メタデータの値をデバッグ出力
    println!("[MP4→CSV] モデルメタデータ:");
    println!("  tile_x: {}, tile_y: {}", metadata.tile_x, metadata.tile_y);
    println!("  tile_width: {}, tile_height: {} (領域全体)", metadata.tile_width, metadata.tile_height);
    println!("  image_width: {}, image_height: {} (個々のタイル)", metadata.image_width, metadata.image_height);
    println!("  columns_per_row: {}", metadata.columns_per_row);
    println!("  button_labels: {:?}", metadata.button_labels);
    
    // 領域全体のサイズを計算（個々のタイルサイズ × 列数）
    // 注意: tile_widthは領域全体の幅、image_widthが個々のタイルサイズ
    let tile_size = metadata.image_width; // 個々のタイルサイズ（48x48）
    let total_width = tile_size * metadata.columns_per_row;
    let total_height = tile_size; // 1行のみ
    
    println!("[MP4→CSV] 計算された領域:");
    println!("  tile_size: {}", tile_size);
    println!("  total_width: {} ({}x{})", total_width, tile_size, metadata.columns_per_row);
    println!("  total_height: {}", total_height);
    
    let region = InputIndicatorRegion {
        x: metadata.tile_x,
        y: metadata.tile_y,
        width: total_width,
        height: total_height,
        rows: 1, // 最下行のみ解析
        cols: metadata.columns_per_row,
    };
    
    println!("[MP4→CSV] InputIndicatorRegion: x={}, y={}, width={}, height={}, rows={}, cols={}",
        region.x, region.y, region.width, region.height, region.rows, region.cols);
    
    // 一時ディレクトリ（システムのtempディレクトリを使用してViteの監視範囲外に配置）
    let temp_dir = std::env::temp_dir().join("input_player_input_extraction");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("一時ディレクトリ作成エラー: {}", e))?;
    let tile_dir = temp_dir.join("tiles");
    fs::create_dir_all(&tile_dir).ok();
    
    // CSV出力はメモリ上でバッファしてから一括書き込みする
    let mut csv_lines: Vec<String> = Vec::new();
    let mut header = vec!["duration".to_string(), "direction".to_string()];
    header.extend(button_labels.clone());
    
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
    // 事前に領域全体を videocrop で切り出してから AppSink で処理する
    extractor.process_frames_sync_with_crop(&video_path, Some(region.clone()), |frame_img, frame_num| {
        total_frames = frame_num + 1;
        
        // 30フレームごとに進捗通知
        if frame_num % 30 == 0 {
            on_progress.send(ExtractionProgress {
                current_frame: frame_num,
                total_frames: 0, // 総フレーム数は不明（動画の最後まで処理しないと分からない）
                message: format!("{}フレーム処理中...", frame_num),
            }).ok();
        }
        
        // AppSinkに渡される画像は既に領域全体でクロップ済みなので、
        // 切り出し後の画像上で列ごとにタイルを抽出する（x=0,y=0開始）
        let cropped_region = crate::analyzer::InputIndicatorRegion {
            x: 0,
            y: 0,
            width: region.width,
            height: region.height,
            rows: region.rows,
            cols: region.cols,
        };

        let tiles = crate::analyzer::extract_tiles_from_image(frame_img, &cropped_region)
            .map_err(|e| anyhow::anyhow!("タイル抽出エラー: {}", e))?;

        // 入力状態を初期化
        let mut current_state = InputState::new();

        // バッチサイズはモデルメタデータの列数を使用
        let batch_size = engine.config().columns_per_row as usize;

        if batch_size == 0 {
            // フォールバック: 個別分類
            for tile in tiles.into_iter() {
                let class_name = engine.classify_image_direct(&tile)
                    .map_err(|e| anyhow::anyhow!("推論エラー: {}", e))?;
                crate::analyzer::update_input_state(&mut current_state, &class_name);
            }
        } else {
            // チャンク毎にバッチ分類を行う
            for chunk in tiles.chunks(batch_size) {
                // chunk は &[image::RgbImage]
                let labels = engine.classify_batch_from_images(chunk)
                    .map_err(|e| anyhow::anyhow!("バッチ推論エラー: {}", e))?;

                for class_name in labels.into_iter() {
                    crate::analyzer::update_input_state(&mut current_state, &class_name);
                }
            }
        }
        
        // 状態が変化したらCSVに書き込み
        if let Some(ref prev) = previous_state {
            if prev != &current_state {
                let line = prev.to_csv_line(duration, &button_labels);
                csv_lines.push(line);
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
    
    // 最後の状態をバッファに追加
    if let Some(ref state) = previous_state {
        let line: String = state.to_csv_line(duration, &button_labels);
        csv_lines.push(line);
    }

    // バッファを書き出す（ヘッダー含む）
    let mut csv_writer = csv::Writer::from_path(&output_csv_path)
        .map_err(|e| format!("CSV作成エラー: {}", e))?;
    csv_writer.write_record(&header)
        .map_err(|e| format!("CSVヘッダー書き込みエラー: {}", e))?;
    for line in csv_lines.into_iter() {
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
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
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
    
    // 学習開始フラグを立てる
    *state.is_training.lock().unwrap() = true;
    
    // ウィンドウのクローズを防止
    if let Some(window) = app_handle.get_webview_window("main") {
        window.set_closable(false).ok();
    }
    
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
    
    // 学習終了後、フラグをクリアしてウィンドウを閉じられるようにする
    *state.is_training.lock().unwrap() = false;
    if let Some(window) = app_handle.get_webview_window("main") {
        window.set_closable(true).ok();
    }
    
    result.map_err(|e| e.to_string())
}

/// タイル分類コマンド（既存タイルの分類）
#[cfg(feature = "ml")]
#[tauri::command]
pub fn classify_video_tiles(
    model_path: String,
    tiles_dir: String,
    output_dir: String,
    use_gpu: bool,
) -> Result<ClassificationResult, String> {
    use crate::ml::classify_tiles;
    
    let classified = classify_tiles(
        PathBuf::from(model_path),
        PathBuf::from(tiles_dir),
        PathBuf::from(output_dir),
        use_gpu,
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

/// 動画からタイルを抽出して分類するコマンド（進捗付き）
#[cfg(feature = "ml")]
#[tauri::command]
pub fn extract_and_classify_tiles(
    video_path: String,
    model_path: String,
    output_dir: String,
    frame_skip: u32,
    use_gpu: bool,
    on_progress: tauri::ipc::Channel<ExtractionProgress>,
) -> Result<ClassificationResult, String> {
    use crate::model::load_metadata;
    use crate::ml::InferenceEngine;
    use std::fs;
    use std::collections::HashMap;
    use gstreamer as gst;
    use gstreamer::prelude::*;
    use gstreamer_app as gst_app;
    use gstreamer_video as gst_video;
    use image::{ImageBuffer, Rgb};
    
    // モデル読み込み（バックエンド設定を使用）
    let engine = InferenceEngine::load_with_backend(&PathBuf::from(&model_path), use_gpu)
        .map_err(|e| format!("モデル読み込みエラー: {}", e))?;
    
    // メタデータ取得
    let metadata = load_metadata(&PathBuf::from(&model_path))
        .map_err(|e| format!("メタデータ読み込みエラー: {}", e))?;
    
    // GStreamer初期化
    gst::init().map_err(|e| format!("GStreamer初期化失敗: {}", e))?;
    
    // 出力ディレクトリ作成（動画名のフォルダ）
    let video_pathbuf = PathBuf::from(&video_path);
    let video_stem = video_pathbuf
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("動画ファイル名の取得エラー")?;
    let video_output_dir = PathBuf::from(&output_dir).join(video_stem);
    
    // クラス毎のディレクトリ作成（all_class_labelsがあればそれを使用）
    let class_labels = if !metadata.all_class_labels.is_empty() {
        &metadata.all_class_labels
    } else {
        &metadata.button_labels
    };
    
    for class_name in class_labels {
        let class_dir = video_output_dir.join(class_name);
        fs::create_dir_all(&class_dir)
            .map_err(|e| format!("ディレクトリ作成エラー: {}", e))?;
    }
    
    // パイプラインを構築
    let pipeline_str = format!(
        "filesrc location=\"{}\" ! decodebin ! videoconvert ! video/x-raw,format=RGB ! appsink name=sink",
        video_path.replace("\\", "/")
    );
    
    let pipeline = gst::parse::launch(&pipeline_str)
        .map_err(|e| format!("パイプライン構築失敗: {}", e))?;
    let pipeline = pipeline
        .dynamic_cast::<gst::Pipeline>()
        .map_err(|_| "Pipeline型への変換失敗")?;
    
    // AppSinkを取得
    let appsink = pipeline
        .by_name("sink")
        .ok_or("AppSinkが見つかりません")?
        .dynamic_cast::<gst_app::AppSink>()
        .map_err(|_| "AppSink型への変換失敗")?;
    
    // パイプラインを開始
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| format!("パイプライン開始失敗: {:?}", e))?;
    
    let mut frame_count = 0u32;
    let mut tile_count: HashMap<String, usize> = HashMap::new();
    let mut total_tiles = 0usize;
    
    // メタデータから動画サイズをチェック
    let expected_width = metadata.video_width as u32;
    let expected_height = metadata.video_height as u32;
    let mut size_checked = false;
    
    // フレームを処理
    loop {
        let sample = match appsink.pull_sample() {
            Ok(sample) => sample,
            Err(_) => break, // EOSまたはエラーで終了
        };
        
        frame_count += 1;
        
        // 間引き処理
        let frame_interval = frame_skip + 1;
        if (frame_count - 1) % frame_interval != 0 {
            continue;
        }
        
        let buffer = sample.buffer().ok_or("バッファ取得失敗")?;
        let caps = sample.caps().ok_or("Caps取得失敗")?;
        
        let video_info = gst_video::VideoInfo::from_caps(caps)
            .map_err(|e| format!("VideoInfo取得失敗: {:?}", e))?;
        
        let width = video_info.width() as u32;
        let height = video_info.height() as u32;
        
        // 動画サイズチェック（初回のみ）
        if !size_checked {
            if width != expected_width || height != expected_height {
                pipeline.set_state(gst::State::Null).ok();
                return Err(format!(
                    "動画サイズが不一致: 動画={}x{}, モデル={}x{}",
                    width, height, expected_width, expected_height
                ));
            }
            size_checked = true;
        }
        
        // 進捗報告（30フレーム毎）
        if frame_count % 30 == 0 {
            on_progress.send(ExtractionProgress {
                current_frame: frame_count,
                total_frames: 0, // 総フレーム数は不明
                message: format!("フレーム {} 処理中 ({} タイル分類済み)...", frame_count, total_tiles),
            }).ok();
        }
        
        // バッファをマップ
        let map = buffer.map_readable()
            .map_err(|_| "バッファマップ失敗")?;
        let data = map.as_slice();
        // 行のバイト幅（stride）を取得して、パディングを考慮したオフセット計算を行う
        let stride_vals = video_info.stride(); // returns &[i32]
        let stride = if let Some(&s) = stride_vals.get(0) {
            let s = if s < 0 { -s } else { s };
            s as usize
        } else {
            // フォールバック: 幅 * 3
            (width as usize) * 3
        };
        
        // 各タイルを切り出して分類（バッチ化）
        // 1行分のタイルをまずメモリ上で収集
        let mut frame_tiles: Vec<image::RgbImage> = Vec::with_capacity(metadata.columns_per_row as usize);
        for col in 0..metadata.columns_per_row {
            let tile_x = metadata.tile_x as u32 + (col as u32 * metadata.tile_width as u32);
            let tile_y = metadata.tile_y as u32; // row == 0

            if tile_x + metadata.tile_width as u32 > width || tile_y + metadata.tile_height as u32 > height {
                // 範囲外はダミータイルを入れずスキップ
                continue;
            }

            let mut tile_img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(
                metadata.tile_width as u32,
                metadata.tile_height as u32,
            );

            for ty in 0..metadata.tile_height as u32 {
                for tx in 0..metadata.tile_width as u32 {
                    let src_x = tile_x + tx;
                    let src_y = tile_y + ty;
                    let src_idx = (src_y as usize * stride) + (src_x as usize * 3);

                    if src_idx + 2 < data.len() {
                        tile_img.put_pixel(tx, ty, Rgb([
                            data[src_idx],
                            data[src_idx + 1],
                            data[src_idx + 2],
                        ]));
                    }
                }
            }

            frame_tiles.push(tile_img);
        }

        // バッチサイズはモデルの列数
        let batch_size = metadata.columns_per_row as usize;

        // 全クラスラベル（出力時に使用）
        let class_labels = if !metadata.all_class_labels.is_empty() {
            &metadata.all_class_labels
        } else {
            &metadata.button_labels
        };

        if batch_size == 0 {
            // フォールバック: 個別分類
            for (i, tile) in frame_tiles.into_iter().enumerate() {
                let class_idx = engine.predict_from_rgb_image(&tile)
                    .map_err(|e| format!("分類エラー: {}", e))?;
                let class_name = class_labels.get(class_idx)
                    .ok_or(format!("クラスインデックス {} が範囲外（クラス数: {}）", class_idx, class_labels.len()))?;

                let tile_id = i + 1;
                let tile_filename = format!("{}_frame={}_tile={}.png", video_stem, frame_count, tile_id);
                let tile_path = video_output_dir.join(class_name).join(&tile_filename);
                let dynamic_img = image::DynamicImage::ImageRgb8(tile);
                save_as_uncompressed_png(&dynamic_img, &tile_path)
                    .map_err(|e| format!("タイル保存エラー: {}", e))?;
                drop(dynamic_img);

                *tile_count.entry(class_name.clone()).or_insert(0) += 1;
                total_tiles += 1;
            }
        } else {
            // チャンク毎にバッチ分類（WGPUなら真のバッチ、NdArrayはチャンク内個別分類にフォールバック）
            for (chunk_idx, chunk) in frame_tiles.chunks(batch_size).enumerate() {
                match &engine {
                    InferenceEngine::Wgpu { .. } => {
                        let labels = engine.classify_batch_from_images(chunk)
                            .map_err(|e| format!("バッチ分類エラー: {}", e))?;

                        for (j, class_name) in labels.into_iter().enumerate() {
                            let tile_index = chunk_idx * batch_size + j;
                            let tile_id = tile_index + 1;
                            // 範囲チェック
                            if tile_index >= frame_tiles.len() { continue; }

                            let tile = &frame_tiles[tile_index];
                            let tile_filename = format!("{}_frame={}_tile={}.png", video_stem, frame_count, tile_id);
                            let tile_path = video_output_dir.join(&class_name).join(&tile_filename);
                            let dynamic_img = image::DynamicImage::ImageRgb8(tile.clone());
                            save_as_uncompressed_png(&dynamic_img, &tile_path)
                                .map_err(|e| format!("タイル保存エラー: {}", e))?;
                            drop(dynamic_img);

                            *tile_count.entry(class_name.clone()).or_insert(0) += 1;
                            total_tiles += 1;
                        }
                    }
                    InferenceEngine::NdArray { .. } => {
                        // CPUでは既存の個別推論をチャンク単位で実行
                        for (j, tile) in chunk.iter().enumerate() {
                            let tile_index = chunk_idx * batch_size + j;
                            let class_name = engine.classify_image_direct(tile)
                                .map_err(|e| format!("分類エラー: {}", e))?;

                            let tile_id = tile_index + 1;
                            let tile_filename = format!("{}_frame={}_tile={}.png", video_stem, frame_count, tile_id);
                            let tile_path = video_output_dir.join(&class_name).join(&tile_filename);
                            let dynamic_img = image::DynamicImage::ImageRgb8(tile.clone());
                            save_as_uncompressed_png(&dynamic_img, &tile_path)
                                .map_err(|e| format!("タイル保存エラー: {}", e))?;
                            drop(dynamic_img);

                            *tile_count.entry(class_name.clone()).or_insert(0) += 1;
                            total_tiles += 1;
                        }
                    }
                }
            }
        }
    }
    
    // パイプラインを停止
    pipeline.set_state(gst::State::Null)
        .map_err(|e| format!("パイプライン停止失敗: {:?}", e))?;
    
    // 最終進捗報告
    on_progress.send(ExtractionProgress {
        current_frame: frame_count,
        total_frames: frame_count,
        message: "分類完了".to_string(),
    }).ok();
    
    // 結果サマリー作成（メタデータの順序でソート、0枚のクラスも含む）
    // 正しい順序: dir_1, dir_2, dir_3, dir_4, dir_6, dir_7, dir_8, dir_9, <ボタンリスト>, others
    let class_labels = if !metadata.all_class_labels.is_empty() {
        &metadata.all_class_labels
    } else {
        // フォールバック: メタデータにall_class_labelsがない場合
        &metadata.button_labels
    };
    
    let summary: Vec<ClassSummary> = class_labels.iter()
        .map(|class_name| ClassSummary {
            class_name: class_name.clone(),
            count: *tile_count.get(class_name).unwrap_or(&0),
        })
        .collect();
    
    Ok(ClassificationResult {
        summary,
        message: format!("タイル分類完了: {} フレーム処理、{} タイル分類", frame_count, total_tiles),
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

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn save_button_order_metadata(_data_dir: String, _button_labels: Vec<String>) -> Result<(), String> {
    Err("機械学習機能が有効化されていません".to_string())
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn load_button_order_metadata(_data_dir: String) -> Result<Option<Vec<String>>, String> {
    Err("機械学習機能が有効化されていません".to_string())
}

/// ボタン順序メタデータを保存
#[cfg(feature = "ml")]
#[tauri::command]
pub fn save_button_order_metadata(data_dir: String, button_labels: Vec<String>) -> Result<(), String> {
    use std::fs;
    use std::path::Path;
    use serde_json;
    
    let data_path = Path::new(&data_dir);
    let metadata_path = data_path.join("button_order.json");
    
    let json = serde_json::to_string_pretty(&button_labels)
        .map_err(|e| format!("JSONシリアライズエラー: {}", e))?;
    
    fs::write(&metadata_path, json)
        .map_err(|e| format!("メタデータファイルの書き込みエラー: {}", e))?;
    
    println!("[ML] ボタン順序メタデータを保存: {:?}", metadata_path);
    Ok(())
}

/// ボタン順序メタデータを読み込み
#[cfg(feature = "ml")]
#[tauri::command]
pub fn load_button_order_metadata(data_dir: String) -> Result<Option<Vec<String>>, String> {
    use std::fs;
    use std::path::Path;
    use serde_json;
    
    let data_path = Path::new(&data_dir);
    let metadata_path = data_path.join("button_order.json");
    
    if !metadata_path.exists() {
        return Ok(None);
    }
    
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("メタデータファイルの読み込みエラー: {}", e))?;
    
    let labels: Vec<String> = serde_json::from_str(&content)
        .map_err(|e| format!("JSONパースエラー: {}", e))?;
    
    println!("[ML] ボタン順序メタデータを読み込み: {:?}", labels);
    Ok(Some(labels))
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

/// MP4動画からシーケンスCSVを生成（進捗通知付き）
/// 
/// extract_input_historyと同じ処理だが、出力パスを自動生成
#[cfg(feature = "ml")]
#[tauri::command]
pub async fn mp4_to_sequence(
    video_path: String,
    model_path: String,
    backend: String,
    on_progress: tauri::ipc::Channel<ExtractionProgress>,
) -> Result<String, String> {
    use std::path::Path;
    
    // 出力CSVパスを生成（動画と同じディレクトリに_input_history.csvを追加）
    let video_path_obj = Path::new(&video_path);
    let stem = video_path_obj.file_stem()
        .ok_or_else(|| "動画ファイル名が無効です".to_string())?
        .to_str()
        .ok_or_else(|| "動画ファイル名のパース失敗".to_string())?;
    
    let parent = video_path_obj.parent()
        .ok_or_else(|| "親ディレクトリが見つかりません".to_string())?;
    
    let output_csv_path = parent.join(format!("{}_input_history.csv", stem));
    let output_csv_str = output_csv_path.to_str()
        .ok_or_else(|| "出力パスの変換失敗".to_string())?
        .to_string();
    
    println!("[MP4→CSV] 開始: {}", video_path);
    println!("[MP4→CSV] 出力: {}", output_csv_str);
    println!("[MP4→CSV] モデル: {}", model_path);
    println!("[MP4→CSV] バックエンド: {}", backend);
    
    // 動画情報を取得して総フレーム数を計算
    use crate::video::FrameExtractor;
    let video_info = FrameExtractor::get_video_info(&video_path)
        .map_err(|e| format!("動画情報取得エラー: {}", e))?;
    let estimated_total_frames = (video_info.duration_sec * video_info.fps) as u32;
    
    println!("[MP4→CSV] 推定総フレーム数: {} ({}秒 × {}fps)", 
        estimated_total_frames, video_info.duration_sec, video_info.fps);
    
    // 初期進捗を送信
    println!("[MP4→CSV] 進捗通知: 推論エンジンを初期化中...");
    on_progress.send(ExtractionProgress {
        current_frame: 0,
        total_frames: estimated_total_frames,
        message: "推論エンジンを初期化中...".to_string(),
    }).ok();
    
    // バックエンド設定
    let use_gpu = backend == "wgpu";
    println!("[MP4→CSV] バックエンド設定: use_gpu={}", use_gpu);
    
    // 推論エンジンを初期化（バックエンド指定）
    println!("[MP4→CSV] InferenceEngine::load_with_backend 呼び出し開始");
    let engine = InferenceEngine::load_with_backend(&PathBuf::from(&model_path), use_gpu)
        .map_err(|e| format!("推論エンジンの初期化エラー: {}", e))?;
    println!("[MP4→CSV] InferenceEngine::load_with_backend 呼び出し完了");
    
    // エンジン初期化完了の通知
    on_progress.send(ExtractionProgress {
        current_frame: 0,
        total_frames: estimated_total_frames,
        message: "モデル読み込み完了。フレーム処理を準備中...".to_string(),
    }).ok();
    
    // メタデータから領域設定を取得
    println!("[MP4→CSV] メタデータ読み込み開始");
    let metadata = load_metadata(&PathBuf::from(&model_path))
        .map_err(|e| format!("メタデータ読み込みエラー: {}", e))?;
    println!("[MP4→CSV] メタデータ読み込み完了");
    
    let button_labels = metadata.button_labels.clone();
    
    // メタデータの値をデバッグ出力
    println!("[MP4→CSV] モデルメタデータ:");
    println!("  tile_x: {}, tile_y: {}", metadata.tile_x, metadata.tile_y);
    println!("  tile_width: {}, tile_height: {} (領域全体)", metadata.tile_width, metadata.tile_height);
    println!("  image_width: {}, image_height: {} (個々のタイル)", metadata.image_width, metadata.image_height);
    println!("  columns_per_row: {}", metadata.columns_per_row);
    println!("  button_labels: {:?}", metadata.button_labels);
    
    // 領域全体のサイズを計算（個々のタイルサイズ × 列数）
    // 注意: tile_widthは領域全体の幅、image_widthが個々のタイルサイズ
    let tile_size = metadata.image_width; // 個々のタイルサイズ（48x48）
    let total_width = tile_size * metadata.columns_per_row;
    let total_height = tile_size; // 1行のみ
    
    println!("[MP4→CSV] 計算された領域:");
    println!("  tile_size: {}", tile_size);
    println!("  total_width: {} ({}x{})", total_width, tile_size, metadata.columns_per_row);
    println!("  total_height: {}", total_height);
    
    let region = InputIndicatorRegion {
        x: metadata.tile_x,
        y: metadata.tile_y,
        width: total_width,
        height: total_height,
        rows: 1,
        cols: metadata.columns_per_row,
    };
    
    println!("[MP4→CSV] InputIndicatorRegion: x={}, y={}, width={}, height={}, rows={}, cols={}",
        region.x, region.y, region.width, region.height, region.rows, region.cols);
    
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
    let mut sequence_steps = 0u32; // シーケンスステップ数
    
    // フレーム抽出設定（output_dirは使用しない）
    let frame_config = FrameExtractorConfig {
        frame_interval: 1, // 全フレーム
        output_dir: PathBuf::from("."), // ダミー（使用しない）
        image_format: "png".to_string(),
        jpeg_quality: 95,
    };
    
    let extractor = FrameExtractor::new(frame_config);
    
    println!("[MP4→CSV] フレーム処理開始");
    
    // フレーム処理開始の進捗を送信
    on_progress.send(ExtractionProgress {
        current_frame: 0,
        total_frames: estimated_total_frames,
        message: "フレーム処理を開始...".to_string(),
    }).ok();
    println!("[MP4→CSV] 進捗通知: フレーム処理を開始...");
    
    // 同期処理: フレーム抽出とタイル推論を同じスレッド内で実行
    println!("[MP4→CSV] process_frames_sync 呼び出し開始");
    extractor.process_frames_sync_with_crop(&video_path, Some(region.clone()), |frame_img, frame_num| {
        total_frames = frame_num + 1;
        
        // 最初のフレームで確認ログ
        if frame_num == 0 {
            println!("[MP4→CSV] 最初のフレームを受信");
        }
        
        // 進捗通知
        println!("[MP4→CSV] フレーム {} 処理中 ({}%)", 
            frame_num, 
            (frame_num as f32 / estimated_total_frames as f32 * 100.0) as u32);
        on_progress.send(ExtractionProgress {
            current_frame: frame_num,
            total_frames: estimated_total_frames,
            message: format!("{}フレーム処理中... ({}%)", 
                frame_num, 
                (frame_num as f32 / estimated_total_frames as f32 * 100.0) as u32),
        }).ok();
        
        // AppSinkに渡される画像は既に領域全体でクロップ済み
        let cropped_region = crate::analyzer::InputIndicatorRegion {
            x: 0,
            y: 0,
            width: region.width,
            height: region.height,
            rows: region.rows,
            cols: region.cols,
        };

        if frame_num == 0 {
            println!("[MP4→CSV] フレーム0: タイル抽出開始 (クロップ済み画像)");
        }
        let tiles = crate::analyzer::extract_tiles_from_image(frame_img, &cropped_region)
            .map_err(|e| anyhow::anyhow!("タイル抽出エラー: {}", e))?;
        if frame_num == 0 {
            println!("[MP4→CSV] フレーム0: タイル抽出完了 ({}個)", tiles.len());
        }
        
        // 各タイルを推論（メモリ上で直接処理）
        let mut current_state = InputState::new();
        
        for (i, tile) in tiles.into_iter().enumerate() {
            if frame_num == 0 && i == 0 {
                println!("[MP4→CSV] フレーム0: 最初のタイル処理開始（直接推論）");
            }
            
            // ファイルI/Oなしで直接推論
            let class_name = engine.classify_image_direct(&tile)
                .map_err(|e| anyhow::anyhow!("推論エラー: {}", e))?;
            
            if frame_num == 0 && i == 0 {
                println!("[MP4→CSV] フレーム0: 最初のタイル推論完了 (クラス: {})", class_name);
            }
            
            // 入力状態に反映
            crate::analyzer::update_input_state(&mut current_state, &class_name);
        }
        
        if frame_num == 0 {
            println!("[MP4→CSV] フレーム0: 全タイル処理完了");
        }
        
        // 状態が変化したらCSVに書き込み
        if let Some(ref prev) = previous_state {
            if prev != &current_state {
                let line = prev.to_csv_line(duration, &button_labels);
                csv_writer.write_record(line.split(','))
                    .map_err(|e| anyhow::anyhow!("CSV書き込みエラー: {}", e))?;
                sequence_steps += 1;
                println!("[MP4→CSV] シーケンス#{}: duration={}F ({:.2}秒)", 
                    sequence_steps, duration, duration as f32 / 60.0);
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
        let line: String = state.to_csv_line(duration, &button_labels);
        csv_writer.write_record(line.split(','))
            .map_err(|e| format!("CSV書き込みエラー: {}", e))?;
        sequence_steps += 1;
        println!("[MP4→CSV] シーケンス#{}: duration={}F ({:.2}秒) - 最終ステップ", 
            sequence_steps, duration, duration as f32 / 60.0);
    }
    
    csv_writer.flush()
        .map_err(|e| format!("CSVフラッシュエラー: {}", e))?;
    
    println!("[MP4→CSV] 完了: {}フレーム → {}シーケンスステップ (平均: {:.1}F/ステップ)", 
        total_frames, sequence_steps, total_frames as f32 / sequence_steps.max(1) as f32);
    
    // 完了通知
    on_progress.send(ExtractionProgress {
        current_frame: total_frames,
        total_frames: total_frames,
        message: format!("完了: {}シーケンスステップを生成", sequence_steps),
    }).ok();
    
    Ok(output_csv_str)
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn mp4_to_sequence(
    _video_path: String,
    _model_path: String,
    _backend: String,
) -> Result<String, String> {
    Err("機械学習機能が有効化されていません".to_string())
}

/// マッピング設定と学習データディレクトリのボタンの整合性をチェック
#[cfg(feature = "ml")]
#[tauri::command]
pub fn validate_mapping_and_training_data(
    mapping_path: String,
    data_dir: String,
) -> Result<(), String> {
    use std::fs;
    use std::path::Path;
    use crate::types::ButtonMapping;
    
    // 1. マッピング設定を読み込む
    let mapping_content = fs::read_to_string(&mapping_path)
        .map_err(|e| format!("マッピング設定の読み込みエラー: {}", e))?;
    
    let mapping: ButtonMapping = serde_json::from_str(&mapping_content)
        .map_err(|e| format!("マッピング設定のパースエラー: {}", e))?;
    
    // use_in_sequence = true のボタンのみを抽出
    let mut mapping_buttons: Vec<String> = mapping
        .mapping
        .iter()
        .filter(|btn| btn.use_in_sequence)
        .map(|btn| btn.user_button.clone())
        .collect();
    mapping_buttons.sort();
    
    // 2. 学習データディレクトリのボタンを取得
    let data_path = Path::new(&data_dir);
    let mut data_buttons = Vec::new();
    
    if let Ok(entries) = fs::read_dir(data_path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        let name_str = name.to_string();
                        // dir_1-9とothersは除外
                        if !name_str.starts_with("dir_") && name_str != "others" {
                            data_buttons.push(name_str);
                        }
                    }
                }
            }
        }
    }
    data_buttons.sort();
    
    // 3. 差分をチェック
    let mapping_set: std::collections::HashSet<_> = mapping_buttons.iter().collect();
    let data_set: std::collections::HashSet<_> = data_buttons.iter().collect();
    
    // マッピングにあるが学習データにないボタン
    let missing_in_data: Vec<_> = mapping_buttons
        .iter()
        .filter(|btn| !data_set.contains(btn))
        .cloned()
        .collect();
    
    // 学習データにあるがマッピングにないボタン
    let missing_in_mapping: Vec<_> = data_buttons
        .iter()
        .filter(|btn| !mapping_set.contains(btn))
        .cloned()
        .collect();
    
    // エラーメッセージを構築
    if !missing_in_data.is_empty() || !missing_in_mapping.is_empty() {
        let mut error_msg = String::from("警告: マッピング設定と学習データディレクトリに不一致があります\n\n");
        
        if !missing_in_data.is_empty() {
            error_msg.push_str(&format!(
                "⚠ マッピング設定にあるが、学習データディレクトリに存在しないボタン:\n  {}\n\n",
                missing_in_data.join(", ")
            ));
        }
        
        if !missing_in_mapping.is_empty() {
            error_msg.push_str(&format!(
                "⚠ 学習データディレクトリにあるが、マッピング設定に存在しないボタン:\n  {}\n\n",
                missing_in_mapping.join(", ")
            ));
        }
        
        error_msg.push_str("推奨: マッピング設定と学習データディレクトリのボタンを一致させてください。");
        
        return Err(error_msg);
    }
    
    Ok(())
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn validate_mapping_and_training_data(
    _mapping_path: String,
    _data_dir: String,
) -> Result<(), String> {
    Err("機械学習機能が有効化されていません".to_string())
}
