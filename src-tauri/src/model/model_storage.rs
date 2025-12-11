//! モデルとメタデータの永続化
//!
//! Tar.gz形式でモデルとメタデータを1ファイルに統合して保存・読み込みします。
//!
//! ファイル構成（tar.gz内部）:
//! - metadata.json   - メタデータ（ボタン情報、タイル設定など）
//! - model.bin       - モデルの重み（バイナリ）

#[cfg(feature = "ml")]
use anyhow::{Context, Result};
#[cfg(feature = "ml")]
use std::path::Path;
#[cfg(feature = "ml")]
use std::fs::File;
#[cfg(feature = "ml")]
use std::io::Read;
#[cfg(feature = "ml")]
use tar::{Builder, Archive};
#[cfg(feature = "ml")]
use flate2::Compression;
#[cfg(feature = "ml")]
use flate2::write::GzEncoder;
#[cfg(feature = "ml")]
use flate2::read::GzDecoder;

#[cfg(feature = "ml")]
use crate::model::model_metadata::ModelMetadata;

/// メタデータと共にモデルをTar.gz形式で保存
///
/// 1つのtar.gzファイルに以下を含む：
/// - metadata.json : メタデータ
/// - model.bin : モデルの重み
#[cfg(feature = "ml")]
pub fn save_model_with_metadata(
    output_path: &Path,
    metadata: &ModelMetadata,
    model_binary: &[u8],
) -> Result<()> {
    // output_pathがすでに.tar.gzで終わっている場合はそのまま、そうでなければ拡張子を追加
    let tar_gz_path = if output_path.extension().and_then(|s| s.to_str()) == Some("gz") {
        output_path.to_path_buf()
    } else {
        output_path.with_extension("tar.gz")
    };
    
    // 親ディレクトリが存在しない場合は作成
    if let Some(parent) = tar_gz_path.parent() {
        std::fs::create_dir_all(parent)
            .context(format!("Failed to create parent directory: {:?}", parent))?;
    }
    
    let tar_gz_file = File::create(&tar_gz_path)
        .context(format!("Failed to create tar.gz file: {:?}", tar_gz_path))?;

    // Gzip圧縮を設定
    let encoder = GzEncoder::new(tar_gz_file, Compression::default());
    let mut tar_builder = Builder::new(encoder);

    // メタデータをJSONとして追加
    let json_str = metadata.to_json_string()?;
    let json_bytes = json_str.as_bytes();

    let mut header = tar::Header::new_gnu();
    header.set_path("metadata.json")?;
    header.set_size(json_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar_builder.append(&header, json_bytes)
        .context("Failed to add metadata.json to tar")?;

    // モデルバイナリを追加
    let mut header = tar::Header::new_gnu();
    header.set_path("model.bin")?;
    header.set_size(model_binary.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar_builder.append(&header, model_binary)
        .context("Failed to add model.bin to tar")?;

    // tarアーカイブを完成させる
    tar_builder.finish()
        .context("Failed to finalize tar.gz archive")?;

    Ok(())
}

/// Tar.gzからモデルメタデータを読み込む
#[cfg(feature = "ml")]
pub fn load_metadata(tar_gz_path: &Path) -> Result<ModelMetadata> {
    let tar_gz_file = File::open(tar_gz_path)
        .context(format!("Failed to open tar.gz file: {:?}", tar_gz_path))?;

    let decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(decoder);

    // metadata.jsonを探す
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if path.to_str() == Some("metadata.json") {
            let mut json_str = String::new();
            entry.read_to_string(&mut json_str)?;
            return ModelMetadata::from_json_string(&json_str);
        }
    }

    Err(anyhow::anyhow!("metadata.json not found in tar.gz archive"))
}

/// Tar.gzからモデルバイナリを読み込む
#[cfg(feature = "ml")]
pub fn load_model_binary(tar_gz_path: &Path) -> Result<Vec<u8>> {
    let tar_gz_file = File::open(tar_gz_path)
        .context(format!("Failed to open tar.gz file: {:?}", tar_gz_path))?;

    let decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(decoder);

    // model.binを探す
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if path.to_str() == Some("model.bin") {
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer)?;
            return Ok(buffer);
        }
    }

    Err(anyhow::anyhow!("model.bin not found in tar.gz archive"))
}

/// メタデータとモデルバイナリを共に読み込む
#[cfg(feature = "ml")]
pub fn load_model_with_metadata(tar_gz_path: &Path) -> Result<(ModelMetadata, Vec<u8>)> {
    let tar_gz_file = File::open(tar_gz_path)
        .context(format!("Failed to open tar.gz file: {:?}", tar_gz_path))?;

    let decoder = GzDecoder::new(tar_gz_file);
    let mut archive = Archive::new(decoder);

    let mut metadata_opt: Option<ModelMetadata> = None;
    let mut model_binary_opt: Option<Vec<u8>> = None;

    // 両方のファイルを読み込む
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        match path.to_str() {
            Some("metadata.json") => {
                let mut json_str = String::new();
                entry.read_to_string(&mut json_str)?;
                metadata_opt = Some(ModelMetadata::from_json_string(&json_str)?);
            }
            Some("model.bin") => {
                let mut buffer = Vec::new();
                entry.read_to_end(&mut buffer)?;
                model_binary_opt = Some(buffer);
            }
            _ => {}
        }
    }

    match (metadata_opt, model_binary_opt) {
        (Some(metadata), Some(binary)) => Ok((metadata, binary)),
        (None, _) => Err(anyhow::anyhow!("metadata.json not found in tar.gz archive")),
        (_, None) => Err(anyhow::anyhow!("model.bin not found in tar.gz archive")),
    }
}

/// メタデータをコンソールに表示
#[cfg(feature = "ml")]
pub fn print_metadata_info(metadata: &ModelMetadata) {
    println!("\n=== モデルメタデータ ===");
    println!("ボタンラベル: {}", metadata.button_labels.join(", "));
    println!("学習データ画像サイズ: {}x{}", metadata.image_width, metadata.image_height);
    println!("タイル設定:");
    println!("  開始位置: ({}, {})", metadata.tile_x, metadata.tile_y);
    println!("  サイズ: {}x{}", metadata.tile_width, metadata.tile_height);
    println!("  列数: {}", metadata.columns_per_row);
    println!("モデル入力サイズ: {}x{}", metadata.model_input_size, metadata.model_input_size);
    println!("学習エポック数: {}", metadata.num_epochs);
    println!("学習日時: {}", metadata.trained_at);
    println!("========================");
}
