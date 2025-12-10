pub mod ml_model;
pub mod training;
pub mod inference;

pub use ml_model::{IconClassifier, ModelConfig, NUM_CLASSES, IMAGE_SIZE, CLASS_NAMES, BUTTON_LABELS, load_and_normalize_image};
pub use training::{TileDataset, train_model, classify_tiles};
pub use inference::InferenceEngine;
