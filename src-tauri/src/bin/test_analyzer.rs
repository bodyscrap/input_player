//! 動画解析機能のテスト用バイナリ

use input_player_lib::video::FrameExtractor;
use input_player_lib::model::{load_metadata, print_metadata_info};
use std::path::PathBuf;

fn main() {
    println!("=== Input Analyzer Backend Test ===\n");
    
    // コマンドライン引数を取得
    let args: Vec<String> = std::env::args().collect();
    let model_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from(r"E:\workspace\input_analyzer\models\icon_classifier.tar.gz")
    };
    
    // テスト1: 動画情報取得
    println!("Test 1: 動画情報取得");
    test_video_info();
    
    println!("\n{}\n", "=".repeat(50));
    
    // テスト2: モデルメタデータ読み込み
    println!("Test 2: モデルメタデータ読み込み");
    test_model_metadata(&model_path);
    
    println!("\n{}\n", "=".repeat(50));
    
    // テスト3: フレーム抽出テスト
    println!("Test 3: フレーム抽出テスト");
    test_frame_extraction();
}

fn test_video_info() {
    let video_path = r"E:\workspace\input_analyzer\sample_data\input_sample_01.mp4";
    
    match FrameExtractor::get_video_info(video_path) {
        Ok(info) => {
            println!("✓ 動画情報取得成功:");
            println!("  解像度: {}x{}", info.width, info.height);
            println!("  FPS: {:.2}", info.fps);
            println!("  再生時間: {:.2}秒", info.duration_sec);
            println!("  総フレーム数: {:.0}", info.fps * info.duration_sec);
        }
        Err(e) => {
            eprintln!("✗ エラー: {}", e);
        }
    }
}

fn test_model_metadata(model_path: &PathBuf) {
    match load_metadata(&model_path) {
        Ok(metadata) => {
            println!("✓ モデルメタデータ読み込み成功:");
            print_metadata_info(&metadata);
        }
        Err(e) => {
            eprintln!("✗ エラー: {}", e);
        }
    }
}

fn test_frame_extraction() {
    use input_player_lib::video::FrameExtractorConfig;
    use std::path::PathBuf;
    
    let video_path = r"E:\workspace\input_analyzer\sample_data\input_sample_01.mp4";
    let output_dir = PathBuf::from("test_output/frames");
    
    // 出力ディレクトリをクリーンアップ
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).ok();
    }
    
    let config = FrameExtractorConfig {
        frame_interval: 30, // 30フレームごと（約0.5秒ごと）
        output_dir: output_dir.clone(),
        image_format: "png".to_string(),
        jpeg_quality: 95,
    };
    
    let extractor = FrameExtractor::new(config);
    
    println!("フレーム抽出を開始...");
    println!("（30フレームごとに抽出）");
    
    match extractor.extract_frames(video_path) {
        Ok(paths) => {
            println!("✓ フレーム抽出成功:");
            println!("  抽出フレーム数: {}", paths.len());
            println!("  出力先: {}", output_dir.display());
            
            if let Some(first) = paths.first() {
                println!("  最初のフレーム: {}", first.display());
            }
        }
        Err(e) => {
            eprintln!("✗ エラー: {}", e);
        }
    }
}
