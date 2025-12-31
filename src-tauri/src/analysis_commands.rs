//! 動画解析関連のTauriコマンド

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::video::{FrameExtractor, FrameExtractorConfig};
use crate::model::AppConfig;
#[cfg(feature = "ml")]
use crate::model::{load_metadata, ModelMetadata};

/// 非圧縮PNGとして画像を保存するヘルパー関数
fn save_as_uncompressed_png<P: AsRef<std::path::Path>>(
    img: &image::DynamicImage,
    path: P,
) -> Result<(), image::ImageError> {
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
    // validate frame_interval
    if frame_interval == 0 {
        return Err("frame_interval must be >= 1".to_string());
    }
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
    
    // 動画情報を取得して videocrop のパラメータを計算
    let info = FrameExtractor::get_video_info(&video_path)
        .map_err(|e| format!("動画情報取得に失敗: {}", e))?;

    let left = region.x as i32;
    let top = region.y as i32;
    let crop_w = region.tile_width * region.columns;
    let crop_h = region.tile_height * region.rows;
    let right = (info.width as i32) - (left + crop_w as i32);
    let bottom = (info.height as i32) - (top + crop_h as i32);
    let right = if right < 0 { 0 } else { right };
    let bottom = if bottom < 0 { 0 } else { bottom };

    // パイプラインを構築（事前に領域全体を videocrop で切り出す）
    let pipeline = format!(
        "filesrc location=\"{}\" ! decodebin ! videoconvert ! videocrop left={} right={} top={} bottom={} ! video/x-raw,format=RGB ! appsink name=sink",
        video_path.replace("\\", "/"), left, right, top, bottom
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
            // 行のバイト幅（stride）を考慮
            let stride_vals = video_info.stride();
            let stride = if let Some(&s) = stride_vals.get(0) {
                let s = if s < 0 { -s } else { s };
                s as usize
            } else {
                (width as usize) * 3
            };
            for row in 0..region.rows {
                for col in 0..region.columns {
                    // videocrop により既に領域全体が切り出されているので origin は 0,0
                    let tile_x = col * region.tile_width;
                    let tile_y = row * region.tile_height;
                    
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
                    
                    // ファイル名形式: {動画名}_frame={フレーム}_tile={タイルid}.png
                    let tile_id = row * region.columns + col;
                    let tile_filename = format!(
                        "{}_frame={}_tile={}.png",
                        video_filename, extracted_frame_count, tile_id
                    );
                    let tile_path = output_path.join(&tile_filename);
                    
                    let dynamic_img = image::DynamicImage::ImageRgb8(tile_img);
                    save_as_uncompressed_png(&dynamic_img, &tile_path)
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
        frame_count: extracted_frame_count,
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
    if frame_interval == 0 {
        return Err("frame_interval must be >= 1".to_string());
    }
    // 出力ディレクトリを作成
    let output_path = PathBuf::from(&output_dir);
    std::fs::create_dir_all(&output_path)
        .map_err(|e| format!("出力ディレクトリの作成に失敗: {}", e))?;

    // クロップ領域（領域全体）を計算
    let crop_region = crate::analyzer::InputIndicatorRegion {
        x: region.x,
        y: region.y,
        width: region.tile_width * region.columns,
        height: region.tile_height * region.rows,
        rows: region.rows,
        cols: region.columns,
    };

    let frame_config = FrameExtractorConfig {
        frame_interval,
        output_dir: PathBuf::from("."), // 使用しない
        image_format: "png".to_string(),
        jpeg_quality: 95,
    };

    let extractor = FrameExtractor::new(frame_config);

    let mut tile_count: usize = 0;
    let mut frame_count: u32 = 0;

    // フレームを同期処理し、クロップ済み画像からタイルを保存
    extractor.process_frames_sync_with_crop(&video_path, Some(crop_region.clone()), |frame_img, frame_num| {
        // frame_img は crop_region サイズの画像
        frame_count = frame_num + 1;

        // 列ごとにタイルを切り出して保存
        for row in 0..crop_region.rows {
            for col in 0..crop_region.cols {
                let x = col * region.tile_width;
                let y = row * region.tile_height;

                // 範囲チェック（念のため）
                if x + region.tile_width > frame_img.width() || y + region.tile_height > frame_img.height() {
                    continue;
                }

                let tile = image::imageops::crop_imm(&mut frame_img.clone(), x, y, region.tile_width, region.tile_height).to_image();

                let tile_filename = format!("tile_f{:06}_r{}_c{}.png", frame_num, row, col);
                let tile_path = output_path.join(&tile_filename);

                let dynamic_img = image::DynamicImage::ImageRgb8(tile);
                save_as_uncompressed_png(&dynamic_img, &tile_path)
                    .map_err(|e| anyhow::anyhow!("タイル保存に失敗: {}", e))?;

                tile_count += 1;
            }
        }

        // テスト用途では無限ループ防止等は呼び出し側で制御する
        Ok(())
    }).map_err(|e| format!("フレーム処理エラー: {}", e))?;

    Ok(ExtractTilesResponse {
        tile_count,
        frame_count,
        message: format!("{}フレームから{}個のタイルを抽出しました", frame_count, tile_count),
    })
}

#[derive(Debug, Serialize)]
pub struct ExtractTilesResponse {
    pub tile_count: usize,
    pub frame_count: u32,
    pub message: String,
}

/// デフォルトの分類フォルダを作成（dir_1～dir_9、others、およびuse_in_sequenceがtrueのボタン）
/// include_neutral: trueの場合はdir_5（ニュートラル）も含める
#[tauri::command]
pub fn create_default_classification_folders(
    output_dir: String,
    button_mapping_path: Option<String>,
    include_neutral: Option<bool>,
) -> Result<String, String> {
    let base_path = PathBuf::from(&output_dir);
    
    // デフォルトのクラス：方向キー8種（または9種）+ others
    let mut default_classes = vec![
        "dir_1", "dir_2", "dir_3", "dir_4",
    ];
    
    // ニュートラル画像ありの場合はdir_5を追加
    if include_neutral.unwrap_or(false) {
        default_classes.push("dir_5");
    }
    
    default_classes.extend_from_slice(&[
        "dir_6", "dir_7", "dir_8", "dir_9",
        "others"
    ]);
    
    let mut created_dirs = Vec::new();
    
    // 各ディレクトリを作成
    for class in default_classes {
        let class_dir = base_path.join(class);
        std::fs::create_dir_all(&class_dir)
            .map_err(|e| format!("ディレクトリ {} の作成に失敗: {}", class, e))?;
        created_dirs.push(class.to_string());
    }
    
    // ボタンマッピングからuse_in_sequenceがtrueのボタンフォルダを作成
    if let Some(mapping_path) = button_mapping_path {
        match load_button_mapping_for_folders(&mapping_path) {
            Ok(button_names) => {
                for button_name in button_names {
                    let button_dir = base_path.join(&button_name);
                    std::fs::create_dir_all(&button_dir)
                        .map_err(|e| format!("ディレクトリ {} の作成に失敗: {}", button_name, e))?;
                    created_dirs.push(button_name);
                }
            },
            Err(e) => {
                // エラーがあっても方向キーのフォルダは作成されているので警告として扱う
                eprintln!("ボタンマッピングの読み込みに失敗: {}", e);
            }
        }
    }
    
    Ok(format!("{}個のクラスディレクトリを作成しました: {}", created_dirs.len(), created_dirs.join(", ")))
}

/// ボタンマッピングからuse_in_sequenceがtrueのボタン名を取得
fn load_button_mapping_for_folders(mapping_path: &str) -> Result<Vec<String>, String> {
    use crate::types::ButtonMapping;
    
    println!("[DEBUG] load_button_mapping_for_folders: 入力パス = {}", mapping_path);
    let path = PathBuf::from(mapping_path);
    println!("[DEBUG] load_button_mapping_for_folders: PathBuf = {:?}", path);
    println!("[DEBUG] load_button_mapping_for_folders: exists? = {}", path.exists());
    
    if !path.exists() {
        return Err(format!("ボタンマッピングファイルが見つかりません: {:?}", path));
    }
    
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("ファイル読み込みエラー: {}", e))?;
    
    println!("[DEBUG] load_button_mapping_for_folders: ファイル読み込み成功、サイズ = {} bytes", content.len());
    
    let mapping: ButtonMapping = serde_json::from_str(&content)
        .map_err(|e| format!("JSONパースエラー: {}", e))?;
    
    println!("[DEBUG] load_button_mapping_for_folders: JSONパース成功、mapping.len = {}", mapping.mapping.len());
    
    let button_names: Vec<String> = mapping.mapping
        .iter()
        .filter(|btn| btn.use_in_sequence)
        .map(|btn| btn.user_button.clone())
        .collect();
    
    println!("[DEBUG] load_button_mapping_for_folders: use_in_sequence=true のボタン = {:?}", button_names);
    
    Ok(button_names)
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

/// モデルメタデータを取得
#[cfg(feature = "ml")]
#[tauri::command]
pub fn get_model_metadata(model_path: String) -> Result<ModelMetadata, String> {
    load_metadata(&PathBuf::from(model_path))
        .map_err(|e| format!("メタデータ読み込みエラー: {}", e))
}

#[cfg(not(feature = "ml"))]
#[tauri::command]
pub fn get_model_metadata(_model_path: String) -> Result<String, String> {
    Err("機械学習機能が有効化されていません".to_string())
}
