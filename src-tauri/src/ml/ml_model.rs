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
        Dropout, DropoutConfig, Linear, LinearConfig, PaddingConfig2d, Relu,
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
}

#[cfg(feature = "ml")]
impl ModelConfig {
    /// モデルを初期化
    pub fn init<B: Backend>(&self, device: &B::Device) -> IconClassifier<B> {
        IconClassifier {
            conv1_1: Conv2dConfig::new([3, 32], [3, 3])
                .with_padding(PaddingConfig2d::Explicit(1, 1))
                .init(device),
            conv1_2: Conv2dConfig::new([32, 32], [3, 3])
                .with_padding(PaddingConfig2d::Explicit(1, 1))
                .init(device),
            pool1: MaxPool2dConfig::new([2, 2]).init(),
            conv2_1: Conv2dConfig::new([32, 64], [3, 3])
                .with_padding(PaddingConfig2d::Explicit(1, 1))
                .init(device),
            conv2_2: Conv2dConfig::new([64, 64], [3, 3])
                .with_padding(PaddingConfig2d::Explicit(1, 1))
                .init(device),
            pool2: MaxPool2dConfig::new([2, 2]).init(),
            conv3_1: Conv2dConfig::new([64, 128], [3, 3])
                .with_padding(PaddingConfig2d::Explicit(1, 1))
                .init(device),
            conv3_2: Conv2dConfig::new([128, 128], [3, 3])
                .with_padding(PaddingConfig2d::Explicit(1, 1))
                .init(device),
            pool3: MaxPool2dConfig::new([2, 2]).init(),
            dropout1: DropoutConfig::new(self.dropout).init(),
            fc1: LinearConfig::new(128 * 6 * 6, 256).init(device),
            dropout2: DropoutConfig::new(self.dropout * 0.6).init(),
            fc2: LinearConfig::new(256, self.num_classes).init(device),
            activation: Relu::new(),
        }
    }
}

/// アイコン分類用CNNモデル
///
/// 48x48のRGB画像を14クラスに分類します。
///
/// # アーキテクチャ
/// - Conv1: 3 -> 32 (48x48 -> 24x24)
/// - Conv2: 32 -> 64 (24x24 -> 12x12)
/// - Conv3: 64 -> 128 (12x12 -> 6x6)
/// - FC: 128*6*6 -> 256 -> 14
#[cfg(feature = "ml")]
#[derive(Module, Debug)]
pub struct IconClassifier<B: Backend> {
    // 48x48 -> 24x24
    conv1_1: Conv2d<B>,
    conv1_2: Conv2d<B>,
    pool1: MaxPool2d,

    // 24x24 -> 12x12
    conv2_1: Conv2d<B>,
    conv2_2: Conv2d<B>,
    pool2: MaxPool2d,

    // 12x12 -> 6x6
    conv3_1: Conv2d<B>,
    conv3_2: Conv2d<B>,
    pool3: MaxPool2d,

    // 全結合層
    dropout1: Dropout,
    fc1: Linear<B>,
    dropout2: Dropout,
    fc2: Linear<B>,

    activation: Relu,
}

#[cfg(feature = "ml")]
impl<B: Backend> IconClassifier<B> {
    /// 順伝播
    ///
    /// # 引数
    /// - `images`: バッチ画像 [batch_size, 3, 48, 48]
    ///
    /// # 戻り値
    /// - クラスごとのロジット [batch_size, num_classes]
    pub fn forward(&self, images: Tensor<B, 4>) -> Tensor<B, 2> {
        let [batch_size, _, _, _] = images.dims();

        // 畳み込み層1
        let x = self.conv1_1.forward(images);
        let x = self.activation.forward(x);
        let x = self.conv1_2.forward(x);
        let x = self.activation.forward(x);
        let x = self.pool1.forward(x);

        // 畳み込み層2
        let x = self.conv2_1.forward(x);
        let x = self.activation.forward(x);
        let x = self.conv2_2.forward(x);
        let x = self.activation.forward(x);
        let x = self.pool2.forward(x);

        // 畳み込み層3
        let x = self.conv3_1.forward(x);
        let x = self.activation.forward(x);
        let x = self.conv3_2.forward(x);
        let x = self.activation.forward(x);
        let x = self.pool3.forward(x);

        // 全結合層
        let x = x.reshape([batch_size, 128 * 6 * 6]);
        let x = self.dropout1.forward(x);
        let x = self.fc1.forward(x);
        let x = self.activation.forward(x);
        let x = self.dropout2.forward(x);
        let x = self.fc2.forward(x);

        x
    }

    /// 予測を実行
    ///
    /// # 引数
    /// - `images`: バッチ画像 [batch_size, 3, 48, 48]
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
    /// - `images`: バッチ画像 [batch_size, 3, 48, 48]
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

/// 画像を読み込んで正規化
///
/// ImageNetの平均と標準偏差で正規化します。
///
/// # 引数
/// - `path`: 画像ファイルのパス
///
/// # 戻り値
/// - 正規化されたRGB画像データ (C, H, W) の順で平坦化
#[cfg(feature = "ml")]
pub fn load_and_normalize_image(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    let img = image::open(path)?.to_rgb8();
    let (width, height) = img.dimensions();

    if width != IMAGE_SIZE as u32 || height != IMAGE_SIZE as u32 {
        anyhow::bail!(
            "画像サイズが不正です: {}x{} (期待: {}x{})",
            width,
            height,
            IMAGE_SIZE,
            IMAGE_SIZE
        );
    }

    let mut data = Vec::with_capacity(3 * IMAGE_SIZE * IMAGE_SIZE);

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
