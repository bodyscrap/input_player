//! アプリケーション設定管理モジュール
//!
//! 計算デバイスやモデル設定などをJSON形式で保存・読み込みします。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 計算デバイスの種類
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceType {
    /// WGPU (GPU) バックエンド
    Wgpu,
    /// NdArray (CPU) バックエンド
    Cpu,
}

impl Default for DeviceType {
    fn default() -> Self {
        DeviceType::Wgpu
    }
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceType::Wgpu => write!(f, "WGPU (GPU)"),
            DeviceType::Cpu => write!(f, "CPU (NdArray)"),
        }
    }
}

/// モデル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    /// 使用するモデルファイルのパス
    pub model_path: String,
    /// 分類クラス数
    pub num_classes: usize,
    /// ドロップアウト率
    pub dropout: f64,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            model_path: "models/icon_classifier.bin".to_string(),
            num_classes: 14,
            dropout: 0.5,
        }
    }
}

/// トレーニング設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingSettings {
    /// エポック数
    pub num_epochs: usize,
    /// バッチサイズ
    pub batch_size: usize,
    /// ワーカー数
    pub num_workers: usize,
    /// 学習率
    pub learning_rate: f64,
    /// ランダムシード
    pub seed: u64,
    /// トレーニングデータの割合
    pub train_ratio: f32,
}

impl Default for TrainingSettings {
    fn default() -> Self {
        Self {
            num_epochs: 50,
            batch_size: 8,
            num_workers: 1,
            learning_rate: 1e-3,
            seed: 42,
            train_ratio: 0.8,
        }
    }
}

/// ボタンタイル切り出し範囲情報
///
/// メタデータ用の参考値を保持します。
/// 実際の解析では最新入力行(最下行)の画面座標 (204, 902) から 336x48 ピクセルを使用します。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonTileSettings {
    /// 切り出し画像内での相対X座標（メタデータ用参考値）
    pub x: u32,
    /// 切り出し画像内での相対Y座標（メタデータ用参考値）
    pub y: u32,
    /// タイルサイズ（正方形、ピクセル）
    /// CNNの入力が正方形であるため、幅と高さは同一である必要がある
    pub tile_size: u32,
    /// 解析対象列数: 継続フレーム数を除く列数 (方向キー1 + ボタン5 = 6列)
    /// 実際の列構成: [継続フレーム数] [方向またはボタン×6] = 計7列
    pub columns_per_row: u32,
    /// 学習データ生成時の動画解像度（幅）
    #[serde(default)]
    pub source_video_width: u32,
    /// 学習データ生成時の動画解像度（高さ）
    #[serde(default)]
    pub source_video_height: u32,
}

impl Default for ButtonTileSettings {
    fn default() -> Self {
        Self {
            x: 80,      // メタデータ用参考値: 切り出し画像内での相対X座標
            y: 400,     // メタデータ用参考値: 切り出し画像内での相対Y座標
            tile_size: 48, // タイルサイズ（正方形、48x48ピクセル）
            columns_per_row: 6, // 解析対象の列数: 方向キー + ボタン5種
            source_video_width: 1920,
            source_video_height: 1080,
        }
    }
}

/// アプリケーション設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 計算デバイスの種類
    pub device_type: DeviceType,
    /// モデル設定
    pub model: ModelSettings,
    /// トレーニング設定
    pub training: TrainingSettings,
    /// ボタンタイル設定
    pub button_tile: ButtonTileSettings,
    /// 最後に使用したビデオファイルのパス
    pub last_video_path: Option<String>,
    /// 最後に使用した出力ディレクトリ
    pub last_output_dir: Option<String>,
    /// 学習データ生成の出力フォルダ（前回値を保存）
    #[serde(default)]
    pub training_output_dir: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_type: DeviceType::default(),
            model: ModelSettings::default(),
            training: TrainingSettings::default(),
            button_tile: ButtonTileSettings::default(),
            last_video_path: None,
            last_output_dir: None,
            training_output_dir: None,
        }
    }
}

impl AppConfig {
    /// 設定ファイルのデフォルトパス
    pub fn default_path() -> PathBuf {
        // src-tauriディレクトリの監視を避けるため、親ディレクトリに保存
        PathBuf::from("../config.json")
    }

    /// 設定を読み込む
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// デフォルトパスから設定を読み込む、存在しない場合はデフォルト設定を返す
    pub fn load_or_default() -> Self {
        let path = Self::default_path();
        if path.exists() {
            match Self::load(&path) {
                Ok(config) => {
                    println!("設定ファイルを読み込みました: {}", path.display());
                    config
                }
                Err(e) => {
                    eprintln!(
                        "警告: 設定ファイルの読み込みに失敗しました ({}): {}",
                        path.display(),
                        e
                    );
                    eprintln!("デフォルト設定を使用します");
                    Self::default()
                }
            }
        } else {
            println!("設定ファイルが存在しません。デフォルト設定を使用します");
            Self::default()
        }
    }

    /// 設定を保存する
    pub fn save<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// デフォルトパスに設定を保存する
    pub fn save_default(&self) -> anyhow::Result<()> {
        let path = Self::default_path();
        self.save(&path)?;
        println!("設定ファイルを保存しました: {}", path.display());
        Ok(())
    }

    /// 計算デバイスを設定
    pub fn set_device_type(&mut self, device_type: DeviceType) {
        self.device_type = device_type;
    }

    /// モデルパスを設定
    pub fn set_model_path(&mut self, path: String) {
        self.model.model_path = path;
    }

    /// 最後に使用したビデオファイルのパスを更新
    pub fn update_last_video_path<P: AsRef<Path>>(&mut self, path: P) {
        self.last_video_path = Some(path.as_ref().to_string_lossy().to_string());
    }

    /// 最後に使用した出力ディレクトリを更新
    pub fn update_last_output_dir<P: AsRef<Path>>(&mut self, path: P) {
        self.last_output_dir = Some(path.as_ref().to_string_lossy().to_string());
    }

    /// 設定情報を表示
    pub fn display(&self) {
        println!("=== アプリケーション設定 ===");
        println!("計算デバイス: {}", self.device_type);
        println!("モデルパス: {}", self.model.model_path);
        println!("分類クラス数: {}", self.model.num_classes);
        println!("ドロップアウト率: {}", self.model.dropout);
        println!("\n--- トレーニング設定 ---");
        println!("エポック数: {}", self.training.num_epochs);
        println!("バッチサイズ: {}", self.training.batch_size);
        println!("学習率: {}", self.training.learning_rate);
        println!("シード: {}", self.training.seed);
        println!("\n--- ボタンタイル設定 ---");
        println!("切り出し開始: ({}, {})", self.button_tile.x, self.button_tile.y);
        println!("タイルサイズ: {}x{}", self.button_tile.tile_size, self.button_tile.tile_size);
        println!("列数: {}", self.button_tile.columns_per_row);

        if let Some(ref video) = self.last_video_path {
            println!("\n最後に使用したビデオ: {}", video);
        }
        if let Some(ref output) = self.last_output_dir {
            println!("最後に使用した出力先: {}", output);
        }
        println!("========================\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.device_type, DeviceType::Wgpu);
        assert_eq!(config.model.num_classes, 14);
        assert_eq!(config.training.num_epochs, 50);
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.device_type, deserialized.device_type);
        assert_eq!(config.model.num_classes, deserialized.model.num_classes);
    }

    #[test]
    fn test_device_type_display() {
        assert_eq!(format!("{}", DeviceType::Wgpu), "WGPU (GPU)");
        assert_eq!(format!("{}", DeviceType::Cpu), "CPU (NdArray)");
    }
}
