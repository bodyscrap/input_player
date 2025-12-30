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
#[cfg(feature = "ml")]
use burn_ndarray::{NdArray, NdArrayDevice};
use std::io::Write;

#[cfg(feature = "ml")]
use crate::ml::{IconClassifier, ModelConfig, load_and_normalize_image_with_size};
#[cfg(feature = "ml")]
use crate::model::{load_metadata, load_model_binary, InferenceConfig};

/// 推論エンジン（enum dispatchパターンでバックエンドを切り替え）
#[cfg(feature = "ml")]
pub enum InferenceEngine {
    Wgpu {
        model: IconClassifier<Wgpu>,
        config: InferenceConfig,
        device: WgpuDevice,
    },
    NdArray {
        model: IconClassifier<NdArray>,
        config: InferenceConfig,
    },
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

        // モデル設定
        let model_config = ModelConfig {
            num_classes: config.num_total_classes(),
            dropout: 0.0, // 推論時はドロップアウトなし
            image_size: metadata.image_width as usize,  // メタデータから取得
        };

        // モデルバイナリ読み込み（.mpk形式）
        let model_binary = load_model_binary(model_path.as_ref())?;

        if use_gpu {
            // GPU (WGPU) バックエンド
            let device = WgpuDevice::DiscreteGpu(0);
            let model = model_config.init::<Wgpu>(&device);

            // 一時ファイルに書き出してDefaultFileRecorder(FullPrecision)で読み込む
            let temp_dir = std::env::temp_dir();
            let temp_model_path = temp_dir.join(format!("model_{}.mpk", std::process::id()));
            
            {
                let mut temp_file = std::fs::File::create(&temp_model_path)?;
                temp_file.write_all(&model_binary)?;
            }

            // モデルの重みを復元
            let record = DefaultFileRecorder::<FullPrecisionSettings>::new()
                .load(temp_model_path.clone(), &device)
                .map_err(|e| anyhow::anyhow!("モデル重みの読み込みエラー: {:?}", e))?;

            let _ = std::fs::remove_file(temp_model_path);
            let model = model.load_record(record);
            
            Ok(Self::Wgpu {
                model,
                config,
                device,
            })
        } else {
            // CPU (NdArray) バックエンド
            let device = NdArrayDevice::Cpu;
            let model = model_config.init::<NdArray>(&device);

            // 一時ファイルに書き出してDefaultFileRecorder(FullPrecision)で読み込む
            let temp_dir = std::env::temp_dir();
            let temp_model_path = temp_dir.join(format!("model_{}.mpk", std::process::id()));
            
            {
                let mut temp_file = std::fs::File::create(&temp_model_path)?;
                temp_file.write_all(&model_binary)?;
            }

            // モデルの重みを復元
            let record = DefaultFileRecorder::<FullPrecisionSettings>::new()
                .load(temp_model_path.clone(), &device)
                .map_err(|e| anyhow::anyhow!("モデル重みの読み込みエラー: {:?}", e))?;

            let _ = std::fs::remove_file(temp_model_path);
            let model = model.load_record(record);
            
            Ok(Self::NdArray {
                model,
                config,
            })
        }
    }

    /// 単一画像を分類
    pub fn classify_image<P: AsRef<Path>>(&self, image_path: P) -> Result<String> {
        match self {
            Self::Wgpu { model, config, device } => {
                let img_size = config.model_input_size as usize;
                let image_data = load_and_normalize_image_with_size(image_path.as_ref(), img_size)?;
                
                let tensor = Tensor::<Wgpu, 1>::from_floats(image_data.as_slice(), device)
                    .reshape([1, 3, img_size, img_size]);
                
                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                let class_idx = predicted
                    .into_data()
                    .to_vec::<i32>()
                    .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?[0] as usize;
                
                let class_name = config.class_index_to_label(class_idx)
                    .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;
                
                Ok(class_name)
            }
            Self::NdArray { model, config } => {
                let img_size = config.model_input_size as usize;
                let image_data = load_and_normalize_image_with_size(image_path.as_ref(), img_size)?;
                
                let device = NdArrayDevice::Cpu;
                let tensor = Tensor::<NdArray, 1>::from_floats(image_data.as_slice(), &device)
                    .reshape([1, 3, img_size, img_size]);
                
                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                let class_idx = predicted
                    .clone()
                    .into_scalar() as usize;
                
                let class_name = config.class_index_to_label(class_idx)
                    .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;
                
                Ok(class_name)
            }
        }
    }

    /// メモリ上の画像を直接分類（ファイルI/Oなし）
    pub fn classify_image_direct(&self, img: &image::RgbImage) -> Result<String> {
        match self {
            Self::Wgpu { model, config, device } => {
                let img_size = config.model_input_size as usize;
                let (width, height) = img.dimensions();
                
                if width != img_size as u32 || height != img_size as u32 {
                    anyhow::bail!(
                        "画像サイズが不正です: {}x{} (期待: {}x{})",
                        width, height, img_size, img_size
                    );
                }

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

                let tensor = Tensor::<Wgpu, 1>::from_floats(data.as_slice(), device)
                    .reshape([1, 3, img_size, img_size]);

                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                let class_idx = predicted
                    .into_data()
                    .to_vec::<i32>()
                    .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?[0] as usize;

                let class_name = config.class_index_to_label(class_idx)
                    .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;

                Ok(class_name)
            }
            Self::NdArray { model, config } => {
                println!("[NdArray推論] 開始");
                let img_size = config.model_input_size as usize;
                let (width, height) = img.dimensions();
                println!("[NdArray推論] サイズ確認: {}x{} (期待: {}x{})", width, height, img_size, img_size);
                
                if width != img_size as u32 || height != img_size as u32 {
                    anyhow::bail!(
                        "画像サイズが不正です: {}x{} (期待: {}x{})",
                        width, height, img_size, img_size
                    );
                }

                println!("[NdArray推論] 正規化開始");
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
                println!("[NdArray推論] 正規化完了");

                println!("[NdArray推論] Tensor作成開始");
                let device = NdArrayDevice::Cpu;
                let tensor = Tensor::<NdArray, 1>::from_floats(data.as_slice(), &device)
                    .reshape([1, 3, img_size, img_size]);
                println!("[NdArray推論] Tensor作成完了");

                println!("[NdArray推論] forward開始");
                let output = model.forward(tensor);
                println!("[NdArray推論] forward完了");
                
                println!("[NdArray推論] argmax開始");
                let predicted = output.argmax(1);
                println!("[NdArray推論] argmax完了");
                
                println!("[NdArray推論] データ取得開始");
                // NdArrayバックエンドではto_vecが遅いため、値を直接取得
                let class_idx = predicted
                    .clone()
                    .into_scalar() as usize;
                println!("[NdArray推論] データ取得完了: class_idx={}", class_idx);

                let class_name = config.class_index_to_label(class_idx)
                    .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;
                println!("[NdArray推論] 完了: {}", class_name);

                Ok(class_name)
            }
        }
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

    /// バッチ画像（RGB画像群）をまとめて分類
    /// images の長さがバッチサイズになります。モデルのメタデータに基づく
    /// 列数などをバッチサイズとして使用してください。
    pub fn classify_batch_from_images(&self, images: &[image::RgbImage]) -> Result<Vec<String>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }

        match self {
            Self::Wgpu { model, config, device } => {
                let img_size = config.model_input_size as usize;
                let batch = images.len();
                let mut normalized = Vec::with_capacity(batch * 3 * img_size * img_size);

                for img in images {
                    let resized = image::imageops::resize(img, img_size as u32, img_size as u32, image::imageops::FilterType::Lanczos3);
                    for channel in 0..3 {
                        for y in 0..img_size {
                            for x in 0..img_size {
                                let pixel = resized.get_pixel(x as u32, y as u32);
                                let value = pixel[channel] as f32 / 255.0;
                                let mean = [0.485f32, 0.456f32, 0.406f32];
                                let std = [0.229f32, 0.224f32, 0.225f32];
                                let normalized_value = (value - mean[channel]) / std[channel];
                                normalized.push(normalized_value);
                            }
                        }
                    }
                }

                let tensor = Tensor::<Wgpu, 1>::from_floats(normalized.as_slice(), device)
                    .reshape([batch, 3, img_size, img_size]);

                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                // 出力の整数型はバックエンドや環境で異なることがあるため、
                // まず i64 を試し、失敗したら i32 を試すフォールバックを行う。
                let mut results = Vec::new();

                // cloneしてi64を試す
                let predicted_clone = predicted.clone();
                match predicted_clone.into_data().to_vec::<i64>() {
                    Ok(vec_i64) => {
                        for idx in vec_i64 {
                            let class_idx = idx as usize;
                            let class_name = config.class_index_to_label(class_idx)
                                .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;
                            results.push(class_name);
                        }
                        return Ok(results);
                    }
                    Err(_) => {
                        // i32 を試す
                        let vec_i32 = predicted
                            .into_data()
                            .to_vec::<i32>()
                            .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?;
                        for idx in vec_i32 {
                            let class_idx = idx as usize;
                            let class_name = config.class_index_to_label(class_idx)
                                .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;
                            results.push(class_name);
                        }
                        return Ok(results);
                    }
                }
            }
            Self::NdArray { model, config } => {
                let img_size = config.model_input_size as usize;
                let batch = images.len();
                let mut normalized = Vec::with_capacity(batch * 3 * img_size * img_size);

                for img in images {
                    let resized = image::imageops::resize(img, img_size as u32, img_size as u32, image::imageops::FilterType::Lanczos3);
                    for channel in 0..3 {
                        for y in 0..img_size {
                            for x in 0..img_size {
                                let pixel = resized.get_pixel(x as u32, y as u32);
                                let value = pixel[channel] as f32 / 255.0;
                                let mean = [0.485f32, 0.456f32, 0.406f32];
                                let std = [0.229f32, 0.224f32, 0.225f32];
                                let normalized_value = (value - mean[channel]) / std[channel];
                                normalized.push(normalized_value);
                            }
                        }
                    }
                }

                let device = NdArrayDevice::Cpu;
                let tensor = Tensor::<NdArray, 1>::from_floats(normalized.as_slice(), &device)
                    .reshape([batch, 3, img_size, img_size]);

                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                let class_idxs = predicted
                    .into_data()
                    .to_vec::<i32>()
                    .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?;

                let mut results = Vec::with_capacity(class_idxs.len());
                for idx in class_idxs {
                    let class_idx = idx as usize;
                    let class_name = config.class_index_to_label(class_idx)
                        .ok_or_else(|| anyhow::anyhow!("クラスインデックス {} は範囲外です", class_idx))?;
                    results.push(class_name);
                }

                Ok(results)
            }
        }
    }

    /// RGB画像から直接分類（クラスインデックスを返す）
    pub fn predict_from_rgb_image(&self, image: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> Result<usize> {
        match self {
            Self::Wgpu { model, config, device } => {
                let img_size = config.model_input_size;
                let resized = image::imageops::resize(
                    image,
                    img_size,
                    img_size,
                    image::imageops::FilterType::Lanczos3
                );
                
                let mean = [0.485, 0.456, 0.406];
                let std = [0.229, 0.224, 0.225];
                
                let img_size_usize = img_size as usize;
                let mut normalized = Vec::with_capacity(3 * img_size_usize * img_size_usize);
                
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
                
                let tensor = Tensor::<Wgpu, 1>::from_floats(normalized.as_slice(), device)
                    .reshape([1, 3, img_size_usize, img_size_usize]);
                
                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                let class_idx = predicted
                    .into_data()
                    .to_vec::<i32>()
                    .map_err(|e| anyhow::anyhow!("推論結果の取得エラー: {:?}", e))?[0] as usize;
                
                Ok(class_idx)
            }
            Self::NdArray { model, config } => {
                let img_size = config.model_input_size;
                let resized = image::imageops::resize(
                    image,
                    img_size,
                    img_size,
                    image::imageops::FilterType::Lanczos3
                );
                
                let mean = [0.485, 0.456, 0.406];
                let std = [0.229, 0.224, 0.225];
                
                let img_size_usize = img_size as usize;
                let mut normalized = Vec::with_capacity(3 * img_size_usize * img_size_usize);
                
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
                
                let device = NdArrayDevice::Cpu;
                let tensor = Tensor::<NdArray, 1>::from_floats(normalized.as_slice(), &device)
                    .reshape([1, 3, img_size_usize, img_size_usize]);
                
                let output = model.forward(tensor);
                let predicted = output.argmax(1);
                let class_idx = predicted
                    .clone()
                    .into_scalar() as usize;
                
                Ok(class_idx)
            }
        }
    }

    /// InferenceConfigへの参照を取得
    pub fn config(&self) -> &InferenceConfig {
        match self {
            Self::Wgpu { config, .. } => config,
            Self::NdArray { config, .. } => config,
        }
    }
}
