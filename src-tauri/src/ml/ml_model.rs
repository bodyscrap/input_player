//! 機械学習モデルの共通定義
//!
//! 入力アイコン分類用のCNNモデルと関連する設定を提供します。

#[cfg(feature = "ml")]
use burn::{
    config::Config,
    module::Module,
    nn::{
        conv::{Conv2d, Conv2dConfig},
        loss::CrossEntropyLossConfig,
        pool::{MaxPool2d, MaxPool2dConfig},
        Linear, LinearConfig, Relu,
    },
    tensor::{backend::Backend, Int, Tensor},
    train::ClassificationOutput,
};

/// クラス数
pub const NUM_CLASSES: usize = 14;

/// 画像サイズ
#[cfg(feature = "ml")]
pub const IMAGE_SIZE: usize = 48;

/// クラス名の定義
pub const CLASS_NAMES: [&str; 14] = [
    "A1", "A2", "B", "Start", "W", "dir_1", "dir_2", "dir_3", "dir_4",
    "dir_6", "dir_7", "dir_8", "dir_9", "others",
];

/// ボタンラベルのみ（方向以外）
pub const BUTTON_LABELS: [&str; 5] = [
    "A1", "A2", "B", "W", "Start",
];

/// モデル設定
#[cfg(feature = "ml")]
#[derive(Config, Debug)]
pub struct ModelConfig {
    /// 分類クラス数
    pub num_classes: usize,
    /// ドロップアウト率
    #[config(default = 0.5)]
    pub dropout: f64,
    /// 入力画像サイズ（正方形）
    #[config(default = 48)]
    pub image_size: usize,
}

#[cfg(feature = "ml")]
impl ModelConfig {
    /// モデルを初期化
    pub fn init<B: Backend>(&self, device: &B::Device) -> IconClassifier<B> {
        // サイズ計算:
        // Conv1 (3x3, no padding): size -> size - 2
        // Pool1 (2x2): (size - 2) -> (size - 2) / 2  (切り捨て)
        // Conv2 (3x3, no padding): ((size - 2) / 2) -> ((size - 2) / 2) - 2
        // Pool2 (2x2): (((size - 2) / 2) - 2) -> (((size - 2) / 2) - 2) / 2
        // Conv3 (3x3, no padding): final_size -> final_size - 2
        
        let after_conv1 = self.image_size.saturating_sub(2);
        let after_pool1 = after_conv1 / 2;
        let after_conv2 = after_pool1.saturating_sub(2);
        let after_pool2 = after_conv2 / 2;
        let feature_map_size = after_pool2.saturating_sub(2);
        
        if feature_map_size == 0 {
            panic!("入力サイズが小さすぎます: {} (最小14x14が必要)", self.image_size);
        }
        
        // 特徴次元 d = 128チャネル * feature_map_size * feature_map_size
        let d = 128 * feature_map_size * feature_map_size;
        let d_half = d / 2;
        
        println!("[Model] 入力サイズ: {}x{}", self.image_size, self.image_size);
        println!("[Model] Conv1後: {}x{}", after_conv1, after_conv1);
        println!("[Model] Pool1後: {}x{}", after_pool1, after_pool1);
        println!("[Model] Conv2後: {}x{}", after_conv2, after_conv2);
        println!("[Model] Pool2後: {}x{}", after_pool2, after_pool2);
        println!("[Model] Conv3後: 128 x {}x{}", feature_map_size, feature_map_size);
        println!("[Model] Flatten後の特徴次元 d: {}", d);
        println!("[Model] FC1: {} -> {}", d, d_half);
        println!("[Model] FC2: {} -> {}", d_half, self.num_classes);
        
        IconClassifier {
            // Conv1: 3x3 (no padding, stride 1)
            conv1: Conv2dConfig::new([3, 32], [3, 3])
                .with_stride([1, 1])
                .init(device),
            pool1: MaxPool2dConfig::new([2, 2]).init(),
            
            // Conv2: 3x3 (no padding, stride 1)
            conv2: Conv2dConfig::new([32, 64], [3, 3])
                .with_stride([1, 1])
                .init(device),
            pool2: MaxPool2dConfig::new([2, 2]).init(),
            
            // Conv3: 3x3 (no padding, stride 1)
            conv3: Conv2dConfig::new([64, 128], [3, 3])
                .with_stride([1, 1])
                .init(device),
            
            // 全結合層
            fc1: LinearConfig::new(d, d_half).init(device),
            fc2: LinearConfig::new(d_half, self.num_classes).init(device),
            
            activation: Relu::new(),
        }
    }
}

/// アイコン分類用CNNモデル
///
/// 任意サイズのRGB画像を任意のクラス数に分類します。
///
/// # アーキテクチャ
/// - {Conv 3x3 (no padding, stride 1) + ReLU} x 3層
/// - Flatten
/// - FC: d -> d/2 + ReLU
/// - FC: d/2 -> num_classes
/// - Softmax (分類時)
///
/// # サイズ計算
/// - padding無し3x3カーネルで、1層でサイズが2減少
/// - 3層後: (size - 6) x (size - 6)
/// - 特徴次元 d = 128 * (size - 6) * (size - 6)
#[cfg(feature = "ml")]
#[derive(Module, Debug)]
pub struct IconClassifier<B: Backend> {
    // 3x3 Conv (no padding) + Max Pooling
    conv1: Conv2d<B>,  // 3 -> 32
    pool1: MaxPool2d,  // 2x2
    conv2: Conv2d<B>,  // 32 -> 64
    pool2: MaxPool2d,  // 2x2
    conv3: Conv2d<B>,  // 64 -> 128

    // 全結合層
    fc1: Linear<B>,    // d -> d/2
    fc2: Linear<B>,    // d/2 -> num_classes

    activation: Relu,
}

#[cfg(feature = "ml")]
impl<B: Backend> IconClassifier<B> {
    /// 順伝播
    ///
    /// # 引数
    /// - `images`: バッチ画像 [batch_size, 3, size, size]
    ///
    /// # 戻り値
    /// - クラスごとのロジット [batch_size, num_classes]
    pub fn forward(&self, images: Tensor<B, 4>) -> Tensor<B, 2> {
        let [batch_size, _, _, _] = images.dims();

        // Conv1: 3x3 (no padding) + ReLU + Pool
        let x = self.conv1.forward(images);
        let x = self.activation.forward(x);
        let x = self.pool1.forward(x);

        // Conv2: 3x3 (no padding) + ReLU + Pool
        let x = self.conv2.forward(x);
        let x = self.activation.forward(x);
        let x = self.pool2.forward(x);

        // Conv3: 3x3 (no padding) + ReLU
        let x = self.conv3.forward(x);
        let x = self.activation.forward(x);

        // Flatten
        let [_, c, h, w] = x.dims();
        let x = x.reshape([batch_size, c * h * w]);

        // FC1: d -> d/2 + ReLU
        let x = self.fc1.forward(x);
        let x = self.activation.forward(x);

        // FC2: d/2 -> num_classes
        let x = self.fc2.forward(x);

        x
    }

    /// 予測を実行
    ///
    /// # 引数
    /// - `images`: バッチ画像 [batch_size, 3, size, size]
    ///
    /// # 戻り値
    /// - (予測クラスID, ロジット)
    pub fn predict(&self, images: Tensor<B, 4>) -> (Tensor<B, 2, burn::tensor::Int>, Tensor<B, 2>) {
        let output = self.forward(images);
        let predictions = output.clone().argmax(1);
        (predictions, output)
    }

    /// 順伝播と損失計算（学習用）
    ///
    /// # 引数
    /// - `images`: バッチ画像 [batch_size, 3, size, size]
    /// - `targets`: ターゲットラベル [batch_size]
    ///
    /// # 戻り値
    /// - ClassificationOutput（損失、出力、ターゲット）
    pub fn forward_classification(
        &self,
        images: Tensor<B, 4>,
        targets: Tensor<B, 1, Int>,
    ) -> ClassificationOutput<B> {
        let output = self.forward(images);
        let loss = CrossEntropyLossConfig::new()
            .init(&output.device())
            .forward(output.clone(), targets.clone());

        ClassificationOutput::new(loss, output, targets)
    }
}

/// 画像を読み込んで正規化（サイズ指定版）
///
/// ImageNetの平均と標準偏差で正規化します。
///
/// # 引数
/// - `path`: 画像ファイルのパス
/// - `expected_size`: 期待する画像サイズ
///
/// # 戻り値
/// - 正規化されたRGB画像データ (C, H, W) の順で平坦化
#[cfg(feature = "ml")]
pub fn load_and_normalize_image_with_size(path: &std::path::Path, expected_size: usize) -> anyhow::Result<Vec<f32>> {
    let img = image::open(path)?.to_rgb8();
    let (width, height) = img.dimensions();

    if width != expected_size as u32 || height != expected_size as u32 {
        anyhow::bail!(
            "画像サイズが不正です: {}x{} (期待: {}x{})",
            width,
            height,
            expected_size,
            expected_size
        );
    }

    let mut data = Vec::with_capacity(3 * expected_size * expected_size);

    // ImageNetの平均と標準偏差で正規化
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

    Ok(data)
}

/// 画像を読み込んで正規化（デフォルトサイズ版）
///
/// 互換性のために残されています。IMAGE_SIZEを使用します。
#[cfg(feature = "ml")]
pub fn load_and_normalize_image(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    load_and_normalize_image_with_size(path, IMAGE_SIZE)
}
