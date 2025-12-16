//! モデルメタデータを使用した推論ツール
//!
//! 保存されたモデルメタデータを読み込んで推論に必要な情報を取得します。

#[cfg(feature = "ml")]
use anyhow::{Context, Result};
#[cfg(feature = "ml")]
use std::path::Path;

#[cfg(feature = "ml")]
use crate::model::model_metadata::ModelMetadata;
#[cfg(feature = "ml")]
use crate::model::model_storage;

/// モデルメタデータから推論用情報を取得
#[cfg(feature = "ml")]
pub struct InferenceConfig {
    /// ボタンラベル一覧
    pub button_labels: Vec<String>,

    /// 全クラスラベル（方向キー + ボタン + others）
    pub all_class_labels: Vec<String>,

    /// 入力画像解像度
    pub image_width: u32,
    pub image_height: u32,

    /// タイル切り出し範囲
    pub tile_x: u32,
    pub tile_y: u32,
    pub tile_width: u32,
    pub tile_height: u32,

    /// 列数
    pub columns_per_row: u32,

    /// モデル入力解像度
    pub model_input_size: u32,
}

#[cfg(feature = "ml")]
impl InferenceConfig {
    /// メタデータからInferenceConfigを作成
    pub fn from_metadata(metadata: &ModelMetadata) -> Self {
        Self {
            button_labels: metadata.button_labels.clone(),
            all_class_labels: metadata.all_class_labels.clone(),
            image_width: metadata.image_width,
            image_height: metadata.image_height,
            tile_x: metadata.tile_x,
            tile_y: metadata.tile_y,
            tile_width: metadata.tile_width,
            tile_height: metadata.tile_height,
            columns_per_row: metadata.columns_per_row,
            model_input_size: metadata.model_input_size,
        }
    }

    /// モデルファイルから推論設定を読み込む
    pub fn load_from_model(model_base_path: &Path) -> Result<Self> {
        let metadata = model_storage::load_metadata(model_base_path)
            .context("Failed to load model metadata")?;
        Ok(Self::from_metadata(&metadata))
    }

    /// ボタンラベルの数を取得
    pub fn num_button_labels(&self) -> usize {
        self.button_labels.len()
    }

    /// 方向を含むすべてのクラス数を取得
    /// all_class_labelsから実際のクラス数を返す
    pub fn num_total_classes(&self) -> usize {
        self.all_class_labels.len()
    }

    /// クラスインデックスからボタンラベルを取得
    ///
    /// インデックスマッピング：
    /// - 0-7: 方向キー（dir_1～dir_9）
    /// - 8以降: ボタン + others
    pub fn class_index_to_label(&self, index: usize) -> Option<String> {
        if index < 8 {
            // 方向キーの場合
            let directions = ["dir_1", "dir_2", "dir_3", "dir_4", "dir_6", "dir_7", "dir_8", "dir_9"];
            Some(directions[index].to_string())
        } else if index < 8 + self.button_labels.len() {
            // ボタンの場合
            let button_idx = index - 8;
            Some(self.button_labels[button_idx].clone())
        } else if index == 8 + self.button_labels.len() {
            // others
            Some("others".to_string())
        } else {
            None
        }
    }

    /// ボタンラベルからクラスインデックスを取得
    pub fn button_label_to_index(&self, label: &str) -> Option<usize> {
        self.button_labels.iter().position(|l| l == label).map(|i| i + 8)
    }

    /// 設定情報を表示
    pub fn print_info(&self) {
        println!("\n=== 推論設定 ===");
        println!("ボタンラベル数: {}", self.button_labels.len());
        println!("ボタンラベル: {}", self.button_labels.join(", "));
        println!("全クラスラベル: {}", self.all_class_labels.join(", "));
        println!("入力画像解像度: {}x{}", self.image_width, self.image_height);
        println!("タイル範囲: ({}, {}) - {}x{}", self.tile_x, self.tile_y, self.tile_width, self.tile_height);
        println!("列数: {}", self.columns_per_row);
        println!("モデル入力サイズ: {}x{}", self.model_input_size, self.model_input_size);
        println!("総クラス数: {}", self.num_total_classes());
        println!("==================");
    }
}
