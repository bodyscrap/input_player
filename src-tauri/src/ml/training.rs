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
use crate::ml::{ModelConfig, IconClassifier, load_and_normalize_image, IMAGE_SIZE};
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
}

#[cfg(feature = "ml")]
impl<B: Backend> TileBatcher<B> {
    pub fn new(device: B::Device) -> Self {
        Self { device }
    }
}

#[cfg(feature = "ml")]
use burn::data::dataloader::batcher::Batcher;

#[cfg(feature = "ml")]
impl<B: Backend> Batcher<B, TileItem, TileBatch<B>> for TileBatcher<B> {
    fn batch(&self, items: Vec<TileItem>, _device: &B::Device) -> TileBatch<B> {
        use burn::tensor::Tensor;
        
        let batch_size = items.len();
        let mut all_pixels = Vec::with_capacity(batch_size * 3 * IMAGE_SIZE * IMAGE_SIZE);
        let mut targets_vec = Vec::with_capacity(batch_size);
        
        for item in items {
            // 画像をロードして正規化（CPUメモリ上）
            match load_and_normalize_image(&item.path) {
                Ok(image_data) => {
                    all_pixels.extend_from_slice(&image_data);
                    targets_vec.push(item.label as i64);
                    // image_dataはここでドロップ（すぐにメモリ解放）
                }
                Err(e) => {
                    eprintln!("警告: 画像読み込み失敗 {}: {}", item.path.display(), e);
                    // エラーの場合はゼロで埋める
                    all_pixels.extend(vec![0.0f32; 3 * IMAGE_SIZE * IMAGE_SIZE]);
                    targets_vec.push(item.label as i64);
                }
            }
        }
        
        // 1回の転送でバッチ全体をGPUメモリへ
        let images = Tensor::<B, 1>::from_floats(all_pixels.as_slice(), &self.device)
            .reshape([batch_size, 3, IMAGE_SIZE, IMAGE_SIZE]);
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
        record::CompactRecorder,
    };
    
    // button_labelsの順序でデータセットを構築
    // クラス順序: dir_1-9(方向キー) -> ユーザーボタン -> others
    let dataset = TileDataset::from_directory_with_order(&data_dir, &button_labels)?;
    
    let total_samples = dataset.len();
    if total_samples == 0 {
        return Err(anyhow::anyhow!("学習データが見つかりません"));
    }
    
    // 学習/検証データに分割 (80/20)
    let (dataset_train, dataset_val) = dataset.split(0.8);
    
    log_callback(format!("学習データ: {} 枚", dataset_train.len()));
    log_callback(format!("検証データ: {} 枚", dataset_val.len()));
    
    // モデル設定
    let num_classes = button_labels.len();
    let model_config = ModelConfig {
        num_classes,
        dropout: 0.5,
    };
    
    log_callback(format!("モデル設定: {} クラス, dropout={}", num_classes, model_config.dropout));
    
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
    let batcher_train = TileBatcher::<burn::backend::Autodiff<Wgpu>>::new(device.clone());
    let batcher_val = TileBatcher::<Wgpu>::new(device.clone());
    
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
    let model_trained = learner.fit(dataloader_train, dataloader_val);
    
    // 最終進捗を報告
    progress_callback(num_epochs, 0.0, 0.0, 0.0, 0.0);
    
    // モデルを保存
    let temp_model_path = PathBuf::from(artifact_dir).join("model");
    model_trained
        .model
        .save_file(&temp_model_path, &CompactRecorder::new())?;
    
    // モデルバイナリを読み込み
    let model_binary = std::fs::read(format!("{}.mpk", temp_model_path.display()))?;
    
    // 保存された解析範囲設定を読み込む
    let config = AppConfig::load_or_default();
    
    // メタデータ作成
    let metadata = ModelMetadata::new(
        button_labels,
        IMAGE_SIZE as u32,
        IMAGE_SIZE as u32,
        config.button_tile.source_video_width,
        config.button_tile.source_video_height,
        config.button_tile.x,
        config.button_tile.y,
        config.button_tile.tile_size,
        config.button_tile.tile_size,
        config.button_tile.columns_per_row,
        IMAGE_SIZE as u32,
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
