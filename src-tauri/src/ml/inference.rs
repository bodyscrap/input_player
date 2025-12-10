//! モデル推論機能

#[cfg(feature = "ml")]
use anyhow::Result;
#[cfg(feature = "ml")]
use std::path::Path;
#[cfg(feature = "ml")]
use burn::{
    backend::Wgpu,
    module::Module,
    record::{BinBytesRecorder, FullPrecisionSettings, Recorder},
    tensor::Tensor,
};

#[cfg(feature = "ml")]
use crate::ml::{IconClassifier, ModelConfig, load_and_normalize_image};
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
        // メタデータ読み込み
        let metadata = load_metadata(model_path.as_ref())?;
        let config = InferenceConfig::from_metadata(&metadata);

        // デバイス設定
        let device = Default::default();

        // モデル設定
        let model_config = ModelConfig {
            num_classes: config.num_total_classes(),
            dropout: 0.0, // 推論時はドロップアウトなし
        };

        // モデル初期化
        let model = model_config.init::<Wgpu>(&device);

        // モデルバイナリ読み込み
        let model_binary = load_model_binary(model_path.as_ref())?;

        // モデルの重みを復元
        let recorder = BinBytesRecorder::<FullPrecisionSettings>::default();
        let record = recorder
            .load(model_binary, &device)
            .map_err(|e| anyhow::anyhow!("モデル重みの読み込みエラー: {:?}", e))?;

        let model = model.load_record(record);

        Ok(Self {
            model,
            config,
            device,
        })
    }

    /// 単一画像を分類
    pub fn classify_image<P: AsRef<Path>>(&self, image_path: P) -> Result<String> {
        // 画像読み込み・正規化
        let image_data = load_and_normalize_image(image_path.as_ref())?;

        // Tensorに変換 [1, 3, 48, 48]
        let tensor = Tensor::<Wgpu, 1>::from_floats(image_data.as_slice(), &self.device)
            .reshape([1, 3, 48, 48]);

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

    /// InferenceConfigへの参照を取得
    pub fn config(&self) -> &InferenceConfig {
        &self.config
    }
}
