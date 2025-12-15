import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./TrainingDialog.css";

interface ModelConfigDialogProps {
  onClose: () => void;
  onModelSet: (modelPath: string) => void;
  currentModelPath: string | null;
  sequenceButtons: string[]; // シーケンスで使用するボタン
}

interface ModelMetadata {
  button_labels: string[];
  all_class_labels: string[];
  image_width: number;
  image_height: number;
  video_width: number;
  video_height: number;
  tile_x: number;
  tile_y: number;
  tile_width: number;
  tile_height: number;
  columns_per_row: number;
  model_input_size: number;
  num_epochs: number;
  trained_at: string;
}

function ModelConfigDialog({
  onClose,
  onModelSet,
  currentModelPath,
  sequenceButtons,
}: ModelConfigDialogProps) {
  const [modelPath, setModelPath] = useState(currentModelPath || "");
  const [metadata, setMetadata] = useState<ModelMetadata | null>(null);
  const [errorMessage, setErrorMessage] = useState<string>("");
  const [isCompatible, setIsCompatible] = useState(false);

  useEffect(() => {
    if (modelPath) {
      loadMetadata(modelPath);
    }
  }, [modelPath]);

  const loadMetadata = async (path: string) => {
    try {
      setErrorMessage("");
      console.log("メタデータ読み込み開始...", path);
      
      const meta = await invoke<ModelMetadata>("get_model_metadata", {
        modelPath: path,
      });
      
      console.log("メタデータ読み込み成功:", meta);
      setMetadata(meta);
      
      // ボタンの互換性チェック
      checkCompatibility(meta);
    } catch (error) {
      console.error("メタデータ読み込みエラー:", error);
      setErrorMessage(`モデルのメタデータを読み込めませんでした: ${error}`);
      setMetadata(null);
      setIsCompatible(false);
    }
  };

  const checkCompatibility = (meta: ModelMetadata) => {
    // モデルのbutton_labelsから方向キーとothersを除外した実際のボタン
    const modelButtons = meta.button_labels.filter(
      label => !label.startsWith("dir_") && label !== "others"
    );
    
    // sequenceButtonsと完全一致するかチェック
    const sortedModelButtons = [...modelButtons].sort();
    const sortedSequenceButtons = [...sequenceButtons].sort();
    
    const matches = 
      sortedModelButtons.length === sortedSequenceButtons.length &&
      sortedModelButtons.every((btn, idx) => btn === sortedSequenceButtons[idx]);
    
    if (matches) {
      setIsCompatible(true);
      setErrorMessage("");
    } else {
      setIsCompatible(false);
      const missing = sequenceButtons.filter(btn => !modelButtons.includes(btn));
      const extra = modelButtons.filter(btn => !sequenceButtons.includes(btn));
      
      let msg = "ボタン設定が一致しません。";
      if (missing.length > 0) {
        msg += `\n不足: ${missing.join(", ")}`;
      }
      if (extra.length > 0) {
        msg += `\n余分: ${extra.join(", ")}`;
      }
      setErrorMessage(msg);
    }
  };

  const handleBrowse = async () => {
    const file = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "Model Files", extensions: ["tar.gz", "gz"] },
      ],
    });
    
    if (file) {
      setModelPath(file);
    }
  };

  const handleSetModel = () => {
    if (modelPath && metadata && isCompatible) {
      onModelSet(modelPath);
      onClose();
    }
  };

  const handleClearModel = () => {
    setModelPath("");
    setMetadata(null);
    setIsCompatible(false);
    setErrorMessage("");
  };

  return (
    <div className="training-dialog-overlay" onClick={onClose}>
      <div className="training-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="training-header">
          <h2>分類モデル設定</h2>
          <button onClick={onClose} className="close-button">✕</button>
        </div>

        <div className="training-content">
          {/* モデル選択 */}
          <div className="config-item">
            <label>モデルファイル</label>
            <div className="input-with-button">
              <input
                type="text"
                value={modelPath}
                readOnly
                placeholder="モデルファイルを選択してください"
              />
              <button onClick={handleBrowse} className="primary-button">
                参照
              </button>
              {modelPath && (
                <button onClick={handleClearModel} className="secondary-button">
                  クリア
                </button>
              )}
            </div>
          </div>

          {/* エラーメッセージ */}
          {errorMessage && (
            <div className="error-banner">
              {errorMessage}
            </div>
          )}

          {/* モデル情報表示 */}
          {metadata && (
            <div className="metadata-info">
              <h4>モデル情報</h4>
              <div className="info-grid">
                <div className="info-item">
                  <span className="info-label">対応ボタン:</span>
                  <span className="info-value">
                    {metadata.button_labels
                      .filter(label => !label.startsWith("dir_") && label !== "others")
                      .join(", ") || "なし"}
                  </span>
                </div>
                <div className="info-item">
                  <span className="info-label">全クラス:</span>
                  <span className="info-value">
                    {metadata.all_class_labels.join(", ")}
                  </span>
                </div>
                <div className="info-item">
                  <span className="info-label">動画サイズ:</span>
                  <span className="info-value">
                    {metadata.video_width} x {metadata.video_height}
                  </span>
                </div>
                <div className="info-item">
                  <span className="info-label">タイル設定:</span>
                  <span className="info-value">
                    {metadata.tile_width} x {metadata.tile_height} (列数: {metadata.columns_per_row})
                  </span>
                </div>
                <div className="info-item">
                  <span className="info-label">学習日時:</span>
                  <span className="info-value">
                    {new Date(metadata.trained_at).toLocaleString("ja-JP")}
                  </span>
                </div>
                <div className="info-item">
                  <span className="info-label">互換性:</span>
                  <span className="info-value" style={{ 
                    color: isCompatible ? "#28a745" : "#dc3545",
                    fontWeight: "bold"
                  }}>
                    {isCompatible ? "✓ 現在のボタン設定と互換性があります" : "✗ 現在のボタン設定と互換性がありません"}
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* 現在のボタン設定 */}
          <div className="metadata-info">
            <h4>現在のボタン設定</h4>
            <div className="info-grid">
              <div className="info-item">
                <span className="info-label">シーケンスボタン:</span>
                <span className="info-value">
                  {sequenceButtons.length > 0 ? sequenceButtons.join(", ") : "なし"}
                </span>
              </div>
            </div>
          </div>
        </div>

        <div className="training-footer">
          <button onClick={onClose} className="secondary-button">
            キャンセル
          </button>
          <button
            onClick={handleSetModel}
            disabled={!isCompatible || !modelPath}
            className="primary-button"
          >
            モデルを設定
          </button>
        </div>
      </div>
    </div>
  );
}

export default ModelConfigDialog;
