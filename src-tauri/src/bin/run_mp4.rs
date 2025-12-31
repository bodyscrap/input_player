//! テスト用: 指定した動画とモデルでタイル分類を行い、分類結果を表示する簡易バイナリ

#[cfg(feature = "ml")]
fn main() {
    use std::path::PathBuf;
    use input_player_lib::video::{FrameExtractor, FrameExtractorConfig};
    use input_player_lib::model::load_metadata;
    use input_player_lib::ml::InferenceEngine;
    use input_player_lib::analyzer::InputIndicatorRegion;

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: run_mp4 <video_path> <model_path> [backend(cpu|wgpu)] [frame_interval]");
        return;
    }

    let video_path = &args[1];
    let model_path = &args[2];
    let backend = if args.len() >= 4 { &args[3] } else { "cpu" };
    let frame_interval: u32 = if args.len() >= 5 {
        args[4].parse().unwrap_or(1)
    } else {
        1
    };

    println!("Run MP4 test:\n  video: {}\n  model: {}\n  backend: {}", video_path, model_path, backend);

    // 初期化
    let use_gpu = backend == "wgpu";
    let engine = match InferenceEngine::load_with_backend(&PathBuf::from(model_path), use_gpu) {
        Ok(e) => e,
        Err(err) => { eprintln!("InferenceEngine init error: {}", err); return; }
    };

    let metadata = match load_metadata(&PathBuf::from(model_path)) {
        Ok(m) => m,
        Err(err) => { eprintln!("load_metadata error: {}", err); return; }
    };

    println!("Model button_labels: {:?}", metadata.button_labels);
    println!("Model all_class_labels: {:?}", metadata.all_class_labels);

    // region 設定
    let tile_size = metadata.image_width;
    let total_width = tile_size * metadata.columns_per_row;
    let region = InputIndicatorRegion {
        x: metadata.tile_x,
        y: metadata.tile_y,
        width: total_width,
        height: tile_size,
        rows: 1,
        cols: metadata.columns_per_row,
    };

    // フレーム間隔はコマンドライン引数で指定可能（デフォルト: 1）
    let out_dir_name = format!("run_mp4_tiles_{}_{}", backend, frame_interval);
    let frame_config = FrameExtractorConfig {
        frame_interval,
        output_dir: PathBuf::from(&out_dir_name),
        image_format: "png".to_string(),
        jpeg_quality: 95,
    };

    let extractor = FrameExtractor::new(frame_config.clone());

    println!("Starting frame processing...");
    let mut frame_count = 0u32;

    if let Err(e) = extractor.process_frames_sync_with_crop(video_path, Some(region.clone()), |frame_img, frame_num| {
        frame_count = frame_num + 1;
        println!("Processing frame {}", frame_num);

        // 事前にクロップ済みの画像上でタイルを抽出（origin は 0,0）
        let cropped_region = input_player_lib::analyzer::InputIndicatorRegion {
            x: 0,
            y: 0,
            width: region.width,
            height: region.height,
            rows: region.rows,
            cols: region.cols,
        };

        let tiles = match input_player_lib::analyzer::extract_tiles_from_image(frame_img, &cropped_region) {
            Ok(t) => t,
            Err(err) => { eprintln!("extract_tiles error: {}", err); return Err(err); }
        };

        // バッチサイズはモデルの列数を使用
        let batch_size = engine.config().columns_per_row as usize;
        let all_tiles = tiles; // Vec<image::RgbImage>
        // 保存先ディレクトリを準備
        let _ = std::fs::create_dir_all(&frame_config.output_dir);

        // 分類処理
        if batch_size == 0 {
            eprintln!("警告: batch_size が 0 です。個別分類にフォールバックします。");
            for (i, tile) in all_tiles.iter().enumerate() {
                match engine.classify_image_direct(tile) {
                    Ok(class_name) => println!(" frame {} tile {} => {}", frame_num, i, class_name),
                    Err(err) => println!(" classification error: {}", err),
                }
            }
        } else {
            for (chunk_idx, chunk) in all_tiles.chunks(batch_size).enumerate() {
                match &engine {
                    InferenceEngine::Wgpu { .. } => {
                        match engine.classify_batch_from_images(chunk) {
                            Ok(labels) => {
                                for (j, class_name) in labels.into_iter().enumerate() {
                                    let tile_index = chunk_idx * batch_size + j;
                                    println!(" frame {} tile {} => {}", frame_num, tile_index, class_name);
                                }
                            }
                            Err(err) => println!(" batch classification error: {}", err),
                        }
                    }
                    InferenceEngine::NdArray { .. } => {
                        // CPUバックエンドでは既存の個別推論を使う（チャンク単位で処理）
                        for (j, tile) in chunk.iter().enumerate() {
                            let tile_index = chunk_idx * batch_size + j;
                            match engine.classify_image_direct(tile) {
                                Ok(class_name) => println!(" frame {} tile {} => {}", frame_num, tile_index, class_name),
                                Err(err) => println!(" classification error: {}", err),
                            }
                        }
                    }
                }
            }
        }

        // タイル画像を保存（テスト目的）
        for (i, tile_img) in all_tiles.into_iter().enumerate() {
            let filename = format!("frame_{:06}_tile_{}.png", frame_num, i);
            let path = frame_config.output_dir.join(filename);
            if let Err(e) = tile_img.save(&path) {
                eprintln!("タイル画像の保存に失敗: {}", e);
            }
        }

        // limit to first few frames for test
        if frame_num >= 5 {
            return Ok(());
        }

        Ok(())
    }) {
        eprintln!("process_frames_sync failed: {}", e);
    } else {
        println!("Done. processed frames: {}", frame_count);
    }
}

#[cfg(not(feature = "ml"))]
fn main() {
    println!("ML機能が有効化されていません");
}
