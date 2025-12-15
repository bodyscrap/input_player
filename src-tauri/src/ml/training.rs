//! モデル学習とタイル分類機能

#[cfg(feature = "ml")]
use anyhow::Result;
#[cfg(feature = "ml")]
use std::path::{Path, PathBuf};
#[cfg(feature = "ml")]
use std::collections::HashMap;

#[cfg(feature = "ml")]
use burn::{
    backend::Wgpu,
    data::dataset::Dataset,
    tensor::{backend::Backend, Int, Tensor},
    module::Module,
};
#[cfg(feature = "ml")]
use burn_wgpu::WgpuDevice;

#[cfg(feature = "ml")]
use crate::ml::{ModelConfig, IconClassifier};
#[cfg(feature = "ml")]
use crate::model::{ModelMetadata, save_model_with_metadata};
#[cfg(feature = "ml")]
use crate::model::config::AppConfig;

/// 学習データセット（パスのリストのみ保持）
#[cfg(feature = "ml")]
pub struct TileDataset {
    samples: Vec<(PathBuf, usize)>, // (画像パス, クラスID)
    class_names: Vec<String>,
    // データセット分割用のインデックス範囲
    start_idx: usize,
    end_idx: usize,
}

#[cfg(feature = "ml")]
impl TileDataset {
    /// 指定された順序でディレクトリから学習データを読み込む
    pub fn from_directory_with_order(data_dir: &Path, class_order: &[String]) -> Result<Self> {
        let mut samples = Vec::new();
        let mut class_map = HashMap::new();
        
        // class_orderに従ってクラスIDを割り当て
        for (class_id, class_name) in class_order.iter().enumerate() {
            class_map.insert(class_name.clone(), class_id);
            
            let class_dir = data_dir.join(class_name);
            if !class_dir.exists() {
                continue; // クラスディレクトリが存在しない場合はスキップ
            }
            
            // クラスディレクトリ内の画像を読み込む
            for entry in std::fs::read_dir(&class_dir)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        if ext_str == "png" || ext_str == "jpg" || ext_str == "jpeg" {
                            samples.push((path, class_id));
                        }
                    }
                }
            }
        }
        
        let len = samples.len();
        Ok(Self {
            samples,
            class_names: class_order.to_vec(),
            start_idx: 0,
            end_idx: len,
        })
    }
    
    /// ディレクトリから学習データを読み込む（旧バージョン・互換性のため残す）
    pub fn from_directory(data_dir: &Path) -> Result<Self> {
        let mut samples = Vec::new();
        let mut class_names = Vec::new();
        let mut class_map = HashMap::new();
        
        // クラスディレクトリを走査
        for entry in std::fs::read_dir(data_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                let class_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| anyhow::anyhow!("Invalid directory name"))?
                    .to_string();
                
                let class_id = class_names.len();
                class_names.push(class_name.clone());
                class_map.insert(class_name.clone(), class_id);
                
                // クラス内の画像を読み込む
                for img_entry in std::fs::read_dir(&path)? {
                    let img_entry = img_entry?;
                    let img_path = img_entry.path();
                    
                    if img_path.is_file() {
                        if let Some(ext) = img_path.extension() {
                            if ext == "png" || ext == "jpg" || ext == "jpeg" {
                                samples.push((img_path, class_id));
                            }
                        }
                    }
                }
            }
        }
        
        if samples.is_empty() {
            anyhow::bail!("No training samples found in {}", data_dir.display());
        }
        
        println!("Loaded {} samples from {} classes", samples.len(), class_names.len());
        for (i, name) in class_names.iter().enumerate() {
            let count = samples.iter().filter(|(_, id)| *id == i).count();
            println!("  Class {}: {} ({} samples)", i, name, count);
        }
        
        let len = samples.len();
        Ok(Self { 
            samples, 
            class_names,
            start_idx: 0,
            end_idx: len,
        })
    }
    
    pub fn class_names(&self) -> &[String] {
        &self.class_names
    }
    
    pub fn num_classes(&self) -> usize {
        self.class_names.len()
    }
    
    /// データセットを学習用と検証用に分割（インデックス範囲のみ）
    pub fn split(self, train_ratio: f32) -> (Self, Self) {
        use rand::seq::SliceRandom;
        use rand::SeedableRng;
        
        // インデックスのみをシャッフル
        let mut indices: Vec<usize> = (0..self.samples.len()).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        indices.shuffle(&mut rng);
        
        // シャッフルされた順序でサンプルを並び替え
        let mut shuffled_samples = Vec::with_capacity(self.samples.len());
        for idx in indices {
            shuffled_samples.push(self.samples[idx].clone());
        }
        
        let total_len = shuffled_samples.len();
        let train_len = (total_len as f32 * train_ratio) as usize;
        
        // 学習用データセット（前半のインデックス範囲）
        let train_dataset = Self {
            samples: shuffled_samples.clone(),
            class_names: self.class_names.clone(),
            start_idx: 0,
            end_idx: train_len,
        };
        
        // 検証用データセット（後半のインデックス範囲）
        let val_dataset = Self {
            samples: shuffled_samples,
            class_names: self.class_names,
            start_idx: train_len,
            end_idx: total_len,
        };
        
        (train_dataset, val_dataset)
    }
}

/// データセットアイテム（画像パスのみ保持）
#[cfg(feature = "ml")]
#[derive(Clone, Debug)]
pub struct TileItem {
    pub path: PathBuf,
    pub label: usize,
}

#[cfg(feature = "ml")]
impl Dataset<TileItem> for TileDataset {
    fn get(&self, index: usize) -> Option<TileItem> {
        // インデックス範囲内のデータのみ返す
        let actual_index = self.start_idx + index;
        if actual_index >= self.end_idx {
            return None;
        }
        
        let (path, label) = self.samples.get(actual_index)?;
        Some(TileItem {
            path: path.clone(),
            label: *label,
        })
    }
    
    fn len(&self) -> usize {
        self.end_idx - self.start_idx
    }
}

/// バッチャー
#[cfg(feature = "ml")]
#[derive(Clone)]
pub struct TileBatcher<B: Backend> {
    device: B::Device,
    tile_size: usize,
}

#[cfg(feature = "ml")]
impl<B: Backend> TileBatcher<B> {
    pub fn new(device: B::Device, tile_size: usize) -> Self {
        Self { device, tile_size }
    }
}

#[cfg(feature = "ml")]
use burn::data::dataloader::batcher::Batcher;

#[cfg(feature = "ml")]
impl<B: Backend> Batcher<B, TileItem, TileBatch<B>> for TileBatcher<B> {
    fn batch(&self, items: Vec<TileItem>, _device: &B::Device) -> TileBatch<B> {
        use burn::tensor::Tensor;
        
        let batch_size = items.len();
        let tile_size = self.tile_size;
        let mut all_pixels = Vec::with_capacity(batch_size * 3 * tile_size * tile_size);
        let mut targets_vec = Vec::with_capacity(batch_size);
        
        for item in items {
            // 画像をロードして正規化（CPUメモリ上）
            match crate::ml::load_and_normalize_image_with_size(&item.path, tile_size) {
                Ok(image_data) => {
                    all_pixels.extend_from_slice(&image_data);
                    targets_vec.push(item.label as i64);
                    // image_dataはここでドロップ（すぐにメモリ解放）
                }
                Err(e) => {
                    eprintln!("警告: 画像読み込み失敗 {}: {}", item.path.display(), e);
                    // エラーの場合はゼロで埋める
                    all_pixels.extend(vec![0.0f32; 3 * tile_size * tile_size]);
                    targets_vec.push(item.label as i64);
                }
            }
        }
        
        // 1回の転送でバッチ全体をGPUメモリへ
        let images = Tensor::<B, 1>::from_floats(all_pixels.as_slice(), &self.device)
            .reshape([batch_size, 3, tile_size, tile_size]);
        let targets = Tensor::<B, 1, Int>::from_ints(targets_vec.as_slice(), &self.device);
        
        // CPUメモリを明示的に解放
        drop(all_pixels);
        drop(targets_vec);
        
        TileBatch { images, targets }
    }
}

/// バッチデータ
#[cfg(feature = "ml")]
#[derive(Clone, Debug)]
pub struct TileBatch<B: Backend> {
    pub images: Tensor<B, 4>,
    pub targets: Tensor<B, 1, Int>,
}

/// TrainStep実装 (学習時の順伝播 + 逆伝播)
#[cfg(feature = "ml")]
impl<B: burn::tensor::backend::AutodiffBackend> burn::train::TrainStep<TileBatch<B>, burn::train::ClassificationOutput<B>> for IconClassifier<B> {
    fn step(&self, batch: TileBatch<B>) -> burn::train::TrainOutput<burn::train::ClassificationOutput<B>> {
        let item = self.forward_classification(batch.images, batch.targets);
        let grads = item.loss.backward();
        burn::train::TrainOutput::new(self, grads, item)
    }
}

/// ValidStep実装 (検証時の順伝播のみ)
#[cfg(feature = "ml")]
impl<B: Backend> burn::train::ValidStep<TileBatch<B>, burn::train::ClassificationOutput<B>> for IconClassifier<B> {
    fn step(&self, batch: TileBatch<B>) -> burn::train::ClassificationOutput<B> {
        self.forward_classification(batch.images, batch.targets)
    }
}

/// モデル学習を実行
/// 
/// button_labelsは以下の順序で構成される:
/// [dir_1～dir_9(方向キー)], [ユーザー定義ボタン], [others]
#[cfg(feature = "ml")]
pub fn train_model<F>(
    data_dir: PathBuf,
    output_model_path: PathBuf,
    num_epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    button_labels: Vec<String>,
    use_gpu: bool,
    _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    mut progress_callback: F,
    log_callback: impl Fn(String) + Send + 'static,
) -> Result<String>
where
    F: FnMut(usize, f64, f64, f64, f64) + Send + 'static,
{
    use burn::{
        data::dataloader::DataLoaderBuilder,
        optim::AdamConfig,
        train::{
            metric::{AccuracyMetric, LossMetric},
            LearnerBuilder, LearningStrategy,
        },
        record::{DefaultFileRecorder, FullPrecisionSettings},
    };
    
    // button_labelsはユーザーボタンのみ（方向キーとothersは含まない）
    // 全クラス順序を構築: dir_1-9(方向キー8個または9個) -> ユーザーボタン -> others
    let mut all_class_labels = vec![
        "dir_1".to_string(), "dir_2".to_string(), "dir_3".to_string(), "dir_4".to_string(),
    ];
    
    // dir_5（ニュートラル）フォルダが存在する場合は含める
    let dir_5_path = data_dir.join("dir_5");
    if dir_5_path.exists() && dir_5_path.is_dir() {
        all_class_labels.push("dir_5".to_string());
        log_callback("dir_5（ニュートラル）フォルダを検出しました".to_string());
    }
    
    all_class_labels.extend_from_slice(&[
        "dir_6".to_string(), "dir_7".to_string(), "dir_8".to_string(), "dir_9".to_string(),
    ]);
    
    // button_labelsから方向キーとothersを除外してユーザーボタンのみを抽出
    let user_buttons: Vec<String> = button_labels
        .iter()
        .filter(|label| !label.starts_with("dir_") && *label != "others")
        .cloned()
        .collect();
    
    all_class_labels.extend(user_buttons.clone());
    all_class_labels.push("others".to_string());
    
    log_callback(format!("ユーザーボタン: {}", user_buttons.join(", ")));
    log_callback(format!("全クラス順序 ({}個): {}", all_class_labels.len(), all_class_labels.join(", ")));
    
    // === バリデーション: タイルサイズチェック ===
    log_callback("学習データの検証を開始します...".to_string());
    
    // 設定ファイルからタイルサイズを取得
    let config = AppConfig::load_or_default();
    let expected_tile_size = config.button_tile.tile_size;
    
    log_callback(format!("現在のタイルサイズ設定: {}x{}", expected_tile_size, expected_tile_size));
    
    // 各クラスディレクトリを検証
    let mut empty_classes = Vec::new();
    let mut invalid_size_images = Vec::new();
    
    for class_name in &all_class_labels {
        let class_dir = data_dir.join(class_name);
        
        if !class_dir.exists() {
            log_callback(format!("警告: クラスディレクトリが存在しません: {}", class_name));
            continue;
        }
        
        // クラス内の画像を収集
        let mut image_count = 0;
        let mut checked_size = false;
        
        for entry in std::fs::read_dir(&class_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if ext_str == "png" || ext_str == "jpg" || ext_str == "jpeg" {
                        image_count += 1;
                        
                        // 最初の画像のサイズをチェック（効率化のため1枚のみ）
                        if !checked_size {
                            match image::open(&path) {
                                Ok(img) => {
                                    let (width, height) = (img.width(), img.height());
                                    if width != expected_tile_size || height != expected_tile_size {
                                        invalid_size_images.push(format!(
                                            "クラス '{}' の画像 '{}': {}x{} (期待: {}x{})",
                                            class_name,
                                            path.file_name().unwrap().to_string_lossy(),
                                            width, height,
                                            expected_tile_size, expected_tile_size
                                        ));
                                    }
                                    checked_size = true;
                                }
                                Err(e) => {
                                    log_callback(format!("警告: 画像読み込み失敗: {} - {}", path.display(), e));
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if image_count == 0 {
            empty_classes.push(class_name.clone());
        }
        
        log_callback(format!("  クラス '{}': {} 枚", class_name, image_count));
    }
    
    // エラーチェック
    if !empty_classes.is_empty() {
        let error_msg = format!(
            "エラー: 以下のクラスディレクトリに画像が1枚もありません:\n{}",
            empty_classes.join(", ")
        );
        log_callback(error_msg.clone());
        return Err(anyhow::anyhow!(error_msg));
    }
    
    if !invalid_size_images.is_empty() {
        let error_msg = format!(
            "エラー: 以下の画像のサイズが現在のタイルサイズ設定({}x{})と一致しません:\n{}",
            expected_tile_size, expected_tile_size,
            invalid_size_images.join("\n")
        );
        log_callback(error_msg.clone());
        return Err(anyhow::anyhow!(error_msg));
    }
    
    log_callback("検証完了: すべてのクラスディレクトリは有効です".to_string());
    
    let dataset = TileDataset::from_directory_with_order(&data_dir, &all_class_labels)?;
    
    let total_samples = dataset.len();
    if total_samples == 0 {
        return Err(anyhow::anyhow!("学習データが見つかりません"));
    }
    
    // 学習/検証データに分割 (80/20)
    let (dataset_train, dataset_val) = dataset.split(0.8);
    
    log_callback(format!("学習データ: {} 枚", dataset_train.len()));
    log_callback(format!("検証データ: {} 枚", dataset_val.len()));
    
    // タイルサイズを取得（検証時に既に取得済み）
    let tile_size = config.button_tile.tile_size as usize;
    
    // モデル設定（全クラス数とタイルサイズを使用）
    let num_classes = all_class_labels.len();
    let model_config = ModelConfig {
        num_classes,
        dropout: 0.5,
        image_size: tile_size,
    };
    
    log_callback(format!("モデル設定: {} クラス, 入力サイズ: {}x{}, dropout={}", 
        num_classes, tile_size, tile_size, model_config.dropout));
    
    // デバイス設定（バックエンド設定に基づく）
    let device = if use_gpu {
        log_callback("GPU (WGPU) モードで学習を開始します".to_string());
        WgpuDevice::DiscreteGpu(0)
    } else {
        log_callback("CPU (WGPU) モードで学習を開始します".to_string());
        WgpuDevice::Cpu
    };
    log_callback(format!("使用デバイス: {:?}", device));
    
    // バッチャー作成
    let batcher_train = TileBatcher::<burn::backend::Autodiff<Wgpu>>::new(device.clone(), tile_size);
    let batcher_val = TileBatcher::<Wgpu>::new(device.clone(), tile_size);
    
    // データローダー作成（num_workers=0でオンデマンド読み込み）
    // データセット分割時に既にシャッフル済みなのでここではシャッフル不要
    let dataloader_train = DataLoaderBuilder::new(batcher_train)
        .batch_size(batch_size)
        .num_workers(0)
        .build(dataset_train);
    
    let dataloader_val = DataLoaderBuilder::new(batcher_val)
        .batch_size(batch_size)
        .num_workers(0)
        .build(dataset_val);
    
    // モデル初期化
    let model = model_config.init::<burn::backend::Autodiff<Wgpu>>(&device);
    
    // アーティファクトディレクトリ作成（Viteの監視対象外）
    let artifact_dir = std::env::temp_dir().join("input_player_training");
    std::fs::create_dir_all(&artifact_dir)?;
    let artifact_dir_str = artifact_dir.to_string_lossy().to_string();
    let artifact_dir_for_cleanup = artifact_dir.clone();
    
    // Learner構築
    log_callback("学習を開始します...".to_string());
    log_callback(format!("エポック数: {}, バッチサイズ: {}, 学習率: {}", num_epochs, batch_size, learning_rate));
    
    let learner = LearnerBuilder::new(&artifact_dir_str)
        .metric_train_numeric(AccuracyMetric::new())
        .metric_valid_numeric(AccuracyMetric::new())
        .metric_train_numeric(LossMetric::new())
        .metric_valid_numeric(LossMetric::new())
        .learning_strategy(LearningStrategy::SingleDevice(device.clone()))
        .num_epochs(num_epochs)
        .summary()
        .build(
            model,
            AdamConfig::new().init(),
            learning_rate,
        );
    
    log_callback("データローダーとモデルの準備が完了しました".to_string());
    
    // TODO: キャンセルフラグと進捗コールバックの統合
    // 現在のburn frameworkではカスタムコールバックが難しいため、
    // 学習完了後にのみ報告
    
    // 学習実行
    log_callback("===learner.fit()を開始します===".to_string());
    eprintln!("[DEBUG] learner.fit()を開始します");
    
    let model_trained = learner.fit(dataloader_train, dataloader_val);
    
    eprintln!("[DEBUG] learner.fit()が完了しました");
    log_callback("===learner.fit()が正常に完了しました===".to_string());
    
    // artifact_dirの内容をデバッグ出力
    eprintln!("[DEBUG] artifact_dir: {}", artifact_dir.display());
    if let Ok(entries) = std::fs::read_dir(&artifact_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                eprintln!("[DEBUG]   - {}: {} bytes", entry.file_name().to_string_lossy(), metadata.len());
            }
        }
    }
    
    // 最終進捗を報告
    progress_callback(num_epochs, 0.0, 0.0, 0.0, 0.0);
    
    log_callback("===進捗報告完了===".to_string());
    
    // モデルを保存
    let temp_model_path = PathBuf::from(artifact_dir).join("model");
    let model_mpk_path = format!("{}.mpk", temp_model_path.display());
    
    log_callback(format!("=== モデル保存処理開始 ==="));
    log_callback(format!("保存先パス: {}", temp_model_path.display()));
    
    // 既存のモデルファイルを削除
    if std::path::Path::new(&model_mpk_path).exists() {
        let old_size = std::fs::metadata(&model_mpk_path)?.len();
        log_callback(format!("既存のモデルファイル発見: {} ({:.2} MB)", 
            model_mpk_path, 
            old_size as f64 / 1024.0 / 1024.0));
        std::fs::remove_file(&model_mpk_path)?;
        log_callback(format!("既存ファイルを削除しました"));
    } else {
        log_callback(format!("既存のモデルファイルなし"));
    }
    
    log_callback(format!("model_trained.modelを保存中..."));
    eprintln!("[DEBUG] save_file()を実行: {}", temp_model_path.display());
    
    // 学習済みモデルを取得
    let trained_model = model_trained.model;
    
    // デバッグ: モデルのパラメータ総数を確認
    let total_params = trained_model.num_params();
    eprintln!("[DEBUG] 学習済みモデルの総パラメータ数: {} ({:.2}M)", total_params, total_params as f64 / 1_000_000.0);
    
    if total_params < 15_000_000 {
        eprintln!("[ERROR] 学習済みモデルのパラメータ数が少なすぎます！期待: 19.8M, 実際: {:.2}M", total_params as f64 / 1_000_000.0);
        eprintln!("[ERROR] これは古いアーキテクチャのモデルです。何らかの理由で学習されたモデルが保持されていません。");
    }
    
    // DefaultFileRecorderを使用してf32精度で保存（CompactRecorderはf16で保存してしまう）
    trained_model.save_file(&temp_model_path, &DefaultFileRecorder::<FullPrecisionSettings>::new())?;
    
    eprintln!("[DEBUG] save_file()完了");
    log_callback(format!("save_file完了"));
    
    // 保存されたファイルサイズを確認
    if std::path::Path::new(&model_mpk_path).exists() {
        let saved_size = std::fs::metadata(&model_mpk_path)?.len();
        eprintln!("[DEBUG] 保存されたmodel.mpkサイズ: {} bytes ({:.2} MB)", saved_size, saved_size as f64 / 1024.0 / 1024.0);
        eprintln!("[DEBUG] 期待サイズ: 79,287,368 bytes (79.29 MB)");
        
        if saved_size < 40_000_000 {
            eprintln!("[ERROR] モデルサイズが小さすぎます！古いアーキテクチャが保存されている可能性があります");
        }
        
        log_callback(format!("保存されたモデルサイズ: {} バイト ({:.2} MB)", 
            saved_size, 
            saved_size as f64 / 1024.0 / 1024.0));
        log_callback(format!("期待サイズ: 79.29 MB (19,821,842 params × 4 bytes)"));
    } else {
        eprintln!("[ERROR] model.mpkファイルが存在しません！");
        log_callback(format!("エラー: モデルファイルが保存されていません！"));
    }
    
    // モデルバイナリを読み込み
    let model_binary_path = format!("{}.mpk", temp_model_path.display());
    log_callback(format!("モデルバイナリパス: {}", model_binary_path));
    
    let model_binary = std::fs::read(&model_binary_path)?;
    log_callback(format!("モデルバイナリサイズ: {} バイト ({:.2} MB)", 
        model_binary.len(), 
        model_binary.len() as f64 / 1024.0 / 1024.0));
    
    // 保存された解析範囲設定を読み込む
    let config = AppConfig::load_or_default();
    
    // メタデータ作成（button_labelsにはユーザーボタンのみ、all_class_labelsに全クラス）
    let tile_size_u32 = config.button_tile.tile_size;
    let metadata = ModelMetadata::new(
        user_buttons,  // ユーザーボタンのみ
        all_class_labels.clone(),  // 全クラス（8方向 + ユーザーボタン + others）
        tile_size_u32,  // 実際のタイルサイズ
        tile_size_u32,  // 実際のタイルサイズ
        config.button_tile.source_video_width,
        config.button_tile.source_video_height,
        config.button_tile.x,
        config.button_tile.y,
        config.button_tile.tile_size,
        config.button_tile.tile_size,
        config.button_tile.columns_per_row,
        tile_size_u32,  // 実際のタイルサイズ
        num_epochs as u32,
    );
    
    // モデルとメタデータを保存
    save_model_with_metadata(&output_model_path, &metadata, &model_binary)?;
    
    // アーティファクトディレクトリをクリーンアップ
    std::fs::remove_dir_all(&artifact_dir_for_cleanup).ok();
    
    Ok(format!("学習完了: {:?} に保存しました", output_model_path))
}

/// タイル分類を実行（学習データフィードバック用）
#[cfg(feature = "ml")]
pub fn classify_tiles(
    _model_path: PathBuf,
    tiles_dir: PathBuf,
    output_dir: PathBuf,
) -> Result<HashMap<String, Vec<PathBuf>>> {
    // TODO: モデル読み込みと推論を実装
    
    let mut classified: HashMap<String, Vec<PathBuf>> = HashMap::new();
    
    // タイルを分類
    for entry in std::fs::read_dir(&tiles_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() {
            // TODO: 実際の分類処理
            let class_name = "others".to_string(); // プレースホルダー
            
            classified.entry(class_name.clone())
                .or_insert_with(Vec::new)
                .push(path.clone());
            
            // 分類結果ディレクトリにコピー
            let class_dir = output_dir.join(&class_name);
            std::fs::create_dir_all(&class_dir)?;
            
            let dest = class_dir.join(path.file_name().unwrap());
            std::fs::copy(&path, &dest)?;
        }
    }
    
    Ok(classified)
}
