//! モデルメタデータの定義と永続化
//!
//! tar.gz形式でモデルと関連するメタデータを保存・読み込みします。
//!
//! ## 入力解析の仕様
//! - 入力インジケータ全体領域: 画面座標 (204, 182) から 336x768 ピクセル (16行×7列)
//! - 解析対象: 最新入力行(最下行、row=15)のみ - 座標 (204, 902) から 336x48 ピクセル
//! - 列構成: [継続フレーム数] [方向キー] [A1] [A2] [B] [W] [Start]
//! - 継続フレーム数は画像から読み取らず、連続する同一入力を独自にカウント

#[cfg(feature = "ml")]
use anyhow::{Context, Result};
#[cfg(feature = "ml")]
use serde::{Deserialize, Serialize};

/// モデルメタデータ
///
/// tar.gz形式で保存される情報：
/// - metadata.json: このメタデータ（JSON形式）
/// - model.bin: モデルの重み（バイナリ）
#[cfg(feature = "ml")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// ボタンラベル（方向キーと"others"は除外）
    /// 例: ["A1", "A2", "B", "W", "Start"]
    pub button_labels: Vec<String>,
    
    /// 全クラスラベル（方向8個 + ボタン + others）
    /// 例: ["dir_1", "dir_2", "dir_3", "dir_4", "dir_6", "dir_7", "dir_8", "dir_9", "A1", "A2", "B", "W", "Start", "others"]
    #[serde(default)]
    pub all_class_labels: Vec<String>,

    /// 学習データの画像幅（ピクセル）
    /// 学習時に使用した画像ファイルから自動検出される（通常48）
    pub image_width: u32,

    /// 学習データの画像高さ（ピクセル）
    /// 学習時に使用した画像ファイルから自動検出される（通常48）
    pub image_height: u32,

    /// 解析対象動画の解像度（幅）
    /// 学習データ生成時の動画解像度を記録（推論時の検証に使用）
    pub video_width: u32,

    /// 解析対象動画の解像度（高さ）
    /// 学習データ生成時の動画解像度を記録（推論時の検証に使用）
    pub video_height: u32,

    /// タイル切り出し開始X座標（切り出し画像内での相対座標）
    /// config.jsonの button_tile.x から取得（デフォルト: 80）
    pub tile_x: u32,

    /// タイル切り出し開始Y座標（切り出し画像内での相対座標）
    /// config.jsonの button_tile.y から取得（デフォルト: 400）
    pub tile_y: u32,

    /// タイルの幅
    /// config.jsonの button_tile.width から取得（デフォルト: 480）
    pub tile_width: u32,

    /// タイルの高さ
    /// config.jsonの button_tile.height から取得（デフォルト: 80）
    pub tile_height: u32,

    /// 解析対象列数: 継続フレーム数を除く列数
    /// config.jsonの button_tile.columns_per_row から取得（デフォルト: 6）
    pub columns_per_row: u32,

    /// モデル入力サイズ（CNNへの入力解像度、通常48x48）
    pub model_input_size: u32,

    /// 学習エポック数
    pub num_epochs: u32,

    /// モデルの学習時刻（ISO8601形式）
    pub trained_at: String,
}

#[cfg(feature = "ml")]
impl ModelMetadata {
    /// 新しいメタデータを作成
    pub fn new(
        button_labels: Vec<String>,
        all_class_labels: Vec<String>,
        image_width: u32,
        image_height: u32,
        video_width: u32,
        video_height: u32,
        tile_x: u32,
        tile_y: u32,
        tile_width: u32,
        tile_height: u32,
        columns_per_row: u32,
        model_input_size: u32,
        num_epochs: u32,
    ) -> Self {
        let trained_at = chrono::Local::now().to_rfc3339();

        Self {
            button_labels,
            all_class_labels,
            image_width,
            image_height,
            video_width,
            video_height,
            tile_x,
            tile_y,
            tile_width,
            tile_height,
            columns_per_row,
            model_input_size,
            num_epochs,
            trained_at,
        }
    }

    /// メタデータをJSON文字列に変換
    pub fn to_json_string(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("Failed to serialize metadata to JSON")
    }

    /// JSON文字列からメタデータを生成
    pub fn from_json_string(json: &str) -> Result<Self> {
        serde_json::from_str(json).context("Failed to deserialize metadata from JSON")
    }
}


