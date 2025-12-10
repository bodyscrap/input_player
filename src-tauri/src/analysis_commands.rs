//! 動画解析関連のTauriコマンド

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::video::{FrameExtractor, FrameExtractorConfig};
use crate::model::AppConfig;

// GStreamer用のインポート
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// 解析範囲設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRegion {
    /// タイル切り出し開始X座標
    pub x: u32,
    /// タイル切り出し開始Y座標  
    pub y: u32,
    /// タイルの幅
    pub tile_width: u32,
    /// タイルの高さ
    pub tile_height: u32,
    /// 列数
    pub columns: u32,
    /// 行数
    pub rows: u32,
    /// 動画の幅
    pub video_width: u32,
    /// 動画の高さ
    pub video_height: u32,
}

/// GStreamerが利用可能かチェック
#[tauri::command]
pub fn check_gstreamer_available() -> Result<(), String> {
    gst::init().map_err(|e| format!("GStreamerが利用できません: {}", e))?;
    Ok(())
}

/// 動画情報取得
#[tauri::command]
pub fn get_video_info(video_path: String) -> Result<VideoInfoResponse, String> {
    let info = FrameExtractor::get_video_info(&video_path)
        .map_err(|e| format!("動画情報の取得に失敗: {}", e))?;
    
    Ok(VideoInfoResponse {
        width: info.width,
        height: info.height,
        fps: info.fps,
        duration_sec: info.duration_sec,
    })
}

#[derive(Debug, Serialize)]
pub struct VideoInfoResponse {
    pub width: i32,
    pub height: i32,
    pub fps: f64,
    pub duration_sec: f64,
}

/// 解析範囲設定を保存
#[tauri::command]
pub fn save_analysis_region(region: AnalysisRegion) -> Result<String, String> {
    let mut config = AppConfig::load_or_default();
    
    config.button_tile.x = region.x;
    config.button_tile.y = region.y;
    config.button_tile.tile_size = region.tile_width; // 正方形を想定
    config.button_tile.columns_per_row = region.columns;
    
    // 動画解像度も保存
    config.button_tile.source_video_width = region.video_width;
    config.button_tile.source_video_height = region.video_height;
    
    config.save_default()
        .map_err(|e| format!("設定の保存に失敗: {}", e))?;
    
    Ok("解析範囲を保存しました".to_string())
}

/// 解析範囲設定を読み込み
#[tauri::command]
pub fn load_analysis_region() -> Result<AnalysisRegion, String> {
    let config = AppConfig::load_or_default();
    
    Ok(AnalysisRegion {
        x: config.button_tile.x,
        y: config.button_tile.y,
        tile_width: config.button_tile.tile_size,
        tile_height: config.button_tile.tile_size,
        columns: config.button_tile.columns_per_row,
        rows: 1, // 最下行のみ解析
        video_width: config.button_tile.source_video_width,
        video_height: config.button_tile.source_video_height,
    })
}

/// 動画から特定フレームを抽出してプレビュー用に返す
#[tauri::command]
pub fn extract_preview_frame(
    video_path: String,
    frame_number: u32,
) -> Result<String, String> {
    use image::ImageEncoder;
    
    // メモリ上でフレームを抽出（ファイル保存なし）
    let config = FrameExtractorConfig::default();
    let extractor = FrameExtractor::new(config);
    let rgb_image = extractor.extract_frame_to_memory(&video_path, frame_number)
        .map_err(|e| format!("フレーム抽出に失敗: {}", e))?;
    
    // PNG形式でメモリ上にエンコード
    let mut png_data = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
    encoder.write_image(
        rgb_image.as_raw(),
        rgb_image.width(),
        rgb_image.height(),
        image::ExtendedColorType::Rgb8,
    ).map_err(|e| format!("PNG エンコードに失敗: {}", e))?;
    
    // Base64エンコード
    let base64_data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png_data);
    
    Ok(format!("data:image/png;base64,{}", base64_data))
}

/// タイル抽出（学習データ生成用）
/// AppSinkを使ってフレームから直接タイルを抽出（学習データ収集用）
#[tauri::command]
pub fn collect_training_data(
    video_path: String,
    output_dir: String,
    frame_interval: u32,
    region: AnalysisRegion,
) -> Result<ExtractTilesResponse, String> {
    use gstreamer_video as gst_video;
    use image::{ImageBuffer, Rgb};
    
    // GStreamerの初期化
    gst::init().map_err(|e| format!("GStreamer初期化失敗: {}", e))?;
    
    // 出力ディレクトリを作成
    let output_path = PathBuf::from(&output_dir);
    std::fs::create_dir_all(&output_path)
        .map_err(|e| format!("出力ディレクトリの作成に失敗: {}", e))?;
    
    // 動画ファイル名を取得（拡張子なし）
    let video_filename = PathBuf::from(&video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video")
        .to_string();
    
    // パイプラインを構築
    let pipeline = format!(
        "filesrc location=\"{}\" ! decodebin ! videoconvert ! video/x-raw,format=RGB ! appsink name=sink",
        video_path.replace("\\", "/")
    );
    
    let pipeline = gst::parse::launch(&pipeline)
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
    let mut tile_count = 0usize;
    let mut extracted_frame_count = 0u32;
    
    // フレームを処理
    loop {
        let sample = match appsink.pull_sample() {
            Ok(sample) => sample,
            Err(_) => break, // EOSまたはエラーで終了
        };
        
        // frame_intervalごとに処理
        if frame_count % frame_interval == 0 {
            let buffer = sample.buffer().ok_or("バッファ取得失敗")?;
            let caps = sample.caps().ok_or("Caps取得失敗")?;
            
            let video_info = gst_video::VideoInfo::from_caps(caps)
                .map_err(|e| format!("VideoInfo取得失敗: {:?}", e))?;
            
            let width = video_info.width() as u32;
            let height = video_info.height() as u32;
            
            // バッファをマップ
            let map = buffer.map_readable()
                .map_err(|_| "バッファマップ失敗")?;
            
            let data = map.as_slice();
            
            // タイルを切り出して保存
            for row in 0..region.rows {
                for col in 0..region.columns {
                    let tile_x = region.x + (col * region.tile_width);
                    let tile_y = region.y + (row * region.tile_height);
                    
                    // 範囲チェック
                    if tile_x + region.tile_width > width || tile_y + region.tile_height > height {
                        continue;
                    }
                    
                    // タイル画像を作成
                    let mut tile_img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(
                        region.tile_width,
                        region.tile_height
                    );
                    
                    for ty in 0..region.tile_height {
                        for tx in 0..region.tile_width {
                            let src_x = tile_x + tx;
                            let src_y = tile_y + ty;
                            let src_idx = ((src_y * width + src_x) * 3) as usize;
                            
                            if src_idx + 2 < data.len() {
                                tile_img.put_pixel(
                                    tx,
                                    ty,
                                    Rgb([data[src_idx], data[src_idx + 1], data[src_idx + 2]])
                                );
                            }
                        }
                    }
                    
                    // ファイル名形式: {動画名}_frame={フレーム}_tile={タイルid}.png
                    let tile_id = row * region.columns + col;
                    let tile_filename = format!(
                        "{}_frame={}_tile={}.png",
                        video_filename, extracted_frame_count, tile_id
                    );
                    let tile_path = output_path.join(&tile_filename);
                    
                    tile_img.save(&tile_path)
                        .map_err(|e| format!("タイル保存失敗: {}", e))?;
                    
                    tile_count += 1;
                }
            }
            
            extracted_frame_count += 1;
        }
        
        frame_count += 1;
    }
    
    // パイプラインを停止
    pipeline
        .set_state(gst::State::Null)
        .map_err(|e| format!("パイプライン停止失敗: {:?}", e))?;
    
    Ok(ExtractTilesResponse {
        tile_count,
        message: format!("{}フレームから{}個のタイルを抽出しました", extracted_frame_count, tile_count),
    })
}

#[tauri::command]
pub fn extract_tiles_from_video(
    video_path: String,
    output_dir: String,
    frame_interval: u32,
    region: AnalysisRegion,
) -> Result<ExtractTilesResponse, String> {
    let config = FrameExtractorConfig {
        frame_interval,
        output_dir: PathBuf::from("temp/frames"),
        image_format: "png".to_string(),
        jpeg_quality: 95,
    };
    
    let extractor = FrameExtractor::new(config);
    
    // フレームを抽出
    let frame_paths = extractor.extract_frames(&video_path)
        .map_err(|e| format!("フレーム抽出に失敗: {}", e))?;
    
    let output_path = PathBuf::from(&output_dir);
    std::fs::create_dir_all(&output_path)
        .map_err(|e| format!("出力ディレクトリの作成に失敗: {}", e))?;
    
    let mut tile_count = 0;
    
    // 各フレームからタイルを切り出し
    for (frame_idx, frame_path) in frame_paths.iter().enumerate() {
        let img = image::open(frame_path)
            .map_err(|e| format!("画像を開けません: {}", e))?;
        
        let tile_width = region.tile_width;
        let tile_height = region.tile_height;
        
        for row in 0..region.rows {
            for col in 0..region.columns {
                let x = region.x + (col * tile_width);
                let y = region.y + (row * tile_height);
                
                // 画像範囲チェック
                if x + tile_width > img.width() || y + tile_height > img.height() {
                    continue;
                }
                
                let tile = img.crop_imm(x, y, tile_width, tile_height);
                
                // タイルを保存（ファイル名に位置情報を含める）
                let tile_filename = format!("tile_f{:06}_r{}_c{}.png", frame_idx, row, col);
                let tile_path = output_path.join(&tile_filename);
                
                tile.save(&tile_path)
                    .map_err(|e| format!("タイル保存に失敗: {}", e))?;
                
                tile_count += 1;
            }
        }
        
        // 元のフレームを削除
        std::fs::remove_file(frame_path).ok();
    }
    
    Ok(ExtractTilesResponse {
        tile_count,
        message: format!("{}個のタイルを抽出しました", tile_count),
    })
}

#[derive(Debug, Serialize)]
pub struct ExtractTilesResponse {
    pub tile_count: usize,
    pub message: String,
}

/// デフォルトの分類フォルダを作成（dir_1～dir_9（dir_5を除く）とothers）
#[tauri::command]
pub fn create_default_classification_folders(
    output_dir: String,
) -> Result<String, String> {
    let base_path = PathBuf::from(&output_dir);
    
    // デフォルトのクラス：方向キー8種 + others（dir_5はニュートラルなので除外）
    let default_classes = vec![
        "dir_1", "dir_2", "dir_3", "dir_4",
        "dir_6", "dir_7", "dir_8", "dir_9",
        "others"
    ];
    
    let mut created_dirs = Vec::new();
    
    // 各ディレクトリを作成
    for class in default_classes {
        let class_dir = base_path.join(class);
        std::fs::create_dir_all(&class_dir)
            .map_err(|e| format!("ディレクトリ {} の作成に失敗: {}", class, e))?;
        created_dirs.push(class.to_string());
    }
    
    Ok(format!("{}個のクラスディレクトリを作成しました", created_dirs.len()))
}

/// 学習用ディレクトリを作成（必須クラスのサブディレクトリを自動生成）
#[tauri::command]
pub fn create_training_directory(
    base_dir: String,
    button_labels: Vec<String>,
) -> Result<String, String> {
    let base_path = PathBuf::from(&base_dir);
    
    // 必須のクラス：方向キー8種 + others
    let required_classes = vec![
        "dir_1", "dir_2", "dir_3", "dir_4",
        "dir_6", "dir_7", "dir_8", "dir_9",
        "others"
    ];
    
    // ベースディレクトリ作成
    std::fs::create_dir_all(&base_path)
        .map_err(|e| format!("ベースディレクトリの作成に失敗: {}", e))?;
    
    let mut created_dirs = Vec::new();
    
    // 方向キーとothersのディレクトリを作成
    for class in required_classes {
        let class_dir = base_path.join(class);
        std::fs::create_dir_all(&class_dir)
            .map_err(|e| format!("ディレクトリ {} の作成に失敗: {}", class, e))?;
        created_dirs.push(class.to_string());
    }
    
    // ボタンラベルのディレクトリを作成
    for label in button_labels {
        let class_dir = base_path.join(&label);
        std::fs::create_dir_all(&class_dir)
            .map_err(|e| format!("ディレクトリ {} の作成に失敗: {}", label, e))?;
        created_dirs.push(label);
    }
    
    Ok(format!("{}個のクラスディレクトリを作成しました: {}", 
        created_dirs.len(), 
        created_dirs.join(", ")))
}
