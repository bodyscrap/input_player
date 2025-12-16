//! モデル推論機能

#[cfg(feature = "ml")]
use anyhow::Result;
#[cfg(feature = "ml")]
use std::path::Path;
#[cfg(feature = "ml")]
use burn::{
    backend::Wgpu,
    module::Module,
    record::{DefaultFileRecorder, FullPrecisionSettings, Recorder},
    tensor::Tensor,
};
#[cfg(feature = "ml")]
use burn_wgpu::WgpuDevice;
use std::io::Write;

#[cfg(feature = "ml")]
use crate::ml::{IconClassifier, ModelConfig, load_and_normalize_image_with_size};
#[cfg(feature = "ml")]
use crate::model::{load_metadata, load_model_binary, InferenceConfig};

/// 推論エンジン
#[cfg(feature = "ml")]
pub struct InferenceEngine {
    model: IconClassifier<Wgpu>,
    config: InferenceConfig,
    device: burn::backend::wgpu::WgpuDevice,
}

#[cfg(feature = "ml")]
impl InferenceEngine {
    /// モデルを読み込んで推論エンジンを初期化
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        Self::load_with_backend(model_path, false) // デフォルトはCPU
    }

    /// モデルを読み込んで推論エンジンを初期化（バックエンド指定）
    pub fn load_with_backend<P: AsRef<Path>>(model_path: P, use_gpu: bool) -> Result<Self> {
        // メタデータ読み込み
        let metadata = load_metadata(model_path.as_ref())?;
        let config = InferenceConfig::from_metadata(&metadata);

        // デバイス設定（バックエンドに基づく）
        let device = if use_gpu {
            WgpuDevice::DiscreteGpu(0)
        } else {
            WgpuDevice::Cpu
        };

        // モデル設定
        let model_config = ModelConfig {
            num_classes: config.num_total_classes(),
            dropout: 0.0, // 推論時はドロップアウトなし
            image_size: metadata.image_width as usize,  // メタデータから取得
        };

        // モデル初期化
        let model = model_config.init::<Wgpu>(&device);

        // モデルバイナリ読み込み（.mpk形式）
        let model_binary = load_model_binary(model_path.as_ref())?;

        // 一時ファイルに書き出してDefaultFileRecorder(FullPrecision)で読み込む
        let temp_dir = std::env::temp_dir();
        let temp_model_path = temp_dir.join(format!("model_{}.mpk", std::process::id()));
        
        {
            let mut temp_file = std::fs::File::create(&temp_model_path)?;
            temp_file.write_all(&model_binary)?;
        }

        // モデルの重みを復元（DefaultFileRecorder<FullPrecisionSettings>を使用 - 学習時と同じ）
        let record = DefaultFileRecorder::<FullPrecisionSettings>::new()
            .load(temp_model_path.clone(), &device)
            .map_err(|e| anyhow::anyhow!("モデル重みの読み込みエラー: {:?}", e))?;

        // 一時ファイルを削除
        let _ = std::fs::remove_file(temp_model_path);

        let model = model.load_record(record);
        Ok(Self {
            model,
            config,
            device,
        })
    }

    /// 単一画像を分類
    pub fn classify_image<P: AsRef<Path>>(&self, image_path: P) -> Result<String> {
        // 画像読み込み・正規化（モデルの期待するサイズを使用）
        let img_size = self.config.model_input_size as usize;
        let image_data = load_and_normalize_image_with_size(image_path.as_ref(), img_size)?;

        // Tensorに変換 [1, 3, img_size, img_size]
        let tensor = Tensor::<Wgpu, 1>::from_floats(image_data.as_slice(), &self.device)
            .reshape([1, 3, img_size, img_size]);

        // 推論実行
        let output = self.model.forward(tensor);
        
        // 最大値のインデックスを取得
        let predicted = output.argmax(1);
        let class_idx = predicted
            .into_data()
            .to_vec::<i32>()
            .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?[0] as usize;

        // クラス名に変換
        let class_name = self.config.class_index_to_label(class_idx)
            .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;

        Ok(class_name)
    }

    /// メモリ上の画像を直接分類（ファイルI/Oなし）
    pub fn classify_image_direct(&self, img: &image::RgbImage) -> Result<String> {
        let img_size = self.config.model_input_size as usize;
        let (width, height) = img.dimensions();
        
        // サイズチェック
        if width != img_size as u32 || height != img_size as u32 {
            anyhow::bail!(
                "画像サイズが不正です: {}x{} (期待: {}x{})",
                width,
                height,
                img_size,
                img_size
            );
        }

        // 正規化
        let mut data = Vec::with_capacity(3 * img_size * img_size);
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];

        for channel in 0..3 {
            for y in 0..height {
                for x in 0..width {
                    let pixel = img.get_pixel(x, y);
                    let value = pixel[channel] as f32 / 255.0;
                    let normalized = (value - mean[channel]) / std[channel];
                    data.push(normalized);
                }
            }
        }

        // Tensorに変換
        let tensor = Tensor::<Wgpu, 1>::from_floats(data.as_slice(), &self.device)
            .reshape([1, 3, img_size, img_size]);

        // 推論実行
        let output = self.model.forward(tensor);
        
        // 最大値のインデックスを取得
        let predicted = output.argmax(1);
        let class_idx = predicted
            .into_data()
            .to_vec::<i32>()
            .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?[0] as usize;

        // クラス名に変換
        let class_name = self.config.class_index_to_label(class_idx)
            .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;

        Ok(class_name)
    }

    /// 複数画像をバッチ分類
    pub fn classify_batch(&self, image_paths: &[impl AsRef<Path>]) -> Result<Vec<String>> {
        let mut results = Vec::new();

        for path in image_paths {
            let class_name = self.classify_image(path)?;
            results.push(class_name);
        }

        Ok(results)
    }

    /// RGB画像から直接分類（クラスインデックスを返す）
    pub fn predict_from_rgb_image(&self, image: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> Result<usize> {
        use burn::tensor::Tensor;
        
        let img_size = self.config.model_input_size;
        // 画像をメタデータで指定されたサイズにリサイズ
        let resized = image::imageops::resize(
            image,
            img_size,
            img_size,
            image::imageops::FilterType::Lanczos3
        );
        
        // ImageNetの平均と標準偏差で正規化（学習時と同じ）
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        
        let img_size_usize = img_size as usize;
        let mut normalized = Vec::with_capacity(3 * img_size_usize * img_size_usize);
        
        // チャネル順: R, G, B
        for channel in 0..3 {
            for y in 0..img_size_usize {
                for x in 0..img_size_usize {
                    let pixel = resized.get_pixel(x as u32, y as u32);
                    let value = pixel[channel] as f32 / 255.0;
                    let normalized_value = (value - mean[channel]) / std[channel];
                    normalized.push(normalized_value);
                }
            }
        }
        
        // Tensorに変換 [1, 3, img_size, img_size]
        let tensor = Tensor::<Wgpu, 1>::from_floats(normalized.as_slice(), &self.device)
            .reshape([1, 3, img_size_usize, img_size_usize]);
        
        // 正規化済みデータを即座に解放
        drop(normalized);
        drop(resized);
        
        // 推論実行（tensorの所有権が移動）
        let output = self.model.forward(tensor);
        
        // 最大値のインデックスを取得（outputの所有権が移動）
        let predicted = output.argmax(1);
        let class_idx = predicted
            .into_data()
            .to_vec::<i32>()
            .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?[0] as usize;
        
        // tensor, output, predictedは所有権移動により自動解放される
        
        Ok(class_idx)
    }

    /// InferenceConfigへの参照を取得
    pub fn config(&self) -> &InferenceConfig {
        &self.config
    }
}
