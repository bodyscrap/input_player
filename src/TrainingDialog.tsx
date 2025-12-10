import { useState, useEffect } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { path } from "@tauri-apps/api";
import "./TrainingDialog.css";

interface TrainingDialogProps {
  mlBackend: "cpu" | "wgpu";
  onClose: () => void;
}

interface TrainingConfig {
  dataDir: string;
  outputDir: string;
  numEpochs: number;
  batchSize: number;
  learningRate: number;
}

interface TrainingProgress {
  current_epoch: number;
  total_epochs: number;
  train_loss: number;
  train_accuracy: number;
  val_loss: number;
  val_accuracy: number;
  message: string;
  log_lines: string[];
}

function TrainingDialog({ mlBackend, onClose }: TrainingDialogProps) {
  const [config, setConfig] = useState<TrainingConfig>({
    dataDir: "",
    outputDir: "",
    numEpochs: 50,
    batchSize: 8,  // input_analyzerと同じデフォルト値（GPUメモリ効率的）
    learningRate: 0.001,
  });

  const [buttonLabels, setButtonLabels] = useState<string[]>([]);
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [isTraining, setIsTraining] = useState(false);
  const [trainingProgress, setTrainingProgress] = useState<TrainingProgress | null>(null);
  const [trainingMessage, setTrainingMessage] = useState<string>("");
  const [trainingComplete, setTrainingComplete] = useState(false);

  // ボタンラベルを自動検出
  useEffect(() => {
    if (config.dataDir) {
      detectButtonLabels();
    }
  }, [config.dataDir]);

  const detectButtonLabels = async () => {
    try {
      // バックエンドからbuttons.txtまたはフォルダ名を取得
      const labels = await invoke<string[]>("get_button_labels_from_data_dir", {
        dataDir: config.dataDir,
      });
      setButtonLabels(labels);
    } catch (error) {
      console.error("ボタンラベルの検出に失敗:", error);
      setButtonLabels([]);
    }
  };

  // ドラッグ&ドロップでボタンラベルを並び替え
  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = "move";
  };

  const handleDragOver = (e: React.DragEvent, _index: number) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  };

  const handleDrop = (e: React.DragEvent, dropIndex: number) => {
    e.preventDefault();
    if (draggedIndex === null || draggedIndex === dropIndex) return;

    const newLabels = [...buttonLabels];
    const [draggedItem] = newLabels.splice(draggedIndex, 1);
    newLabels.splice(dropIndex, 0, draggedItem);

    setButtonLabels(newLabels);
    setDraggedIndex(null);
  };

  const handleDragEnd = () => {
    setDraggedIndex(null);
  };

  const handleSelectDataDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "学習データディレクトリを選択",
      });

      if (selected) {
        setConfig({ ...config, dataDir: selected as string });
      }
    } catch (error) {
      console.error("ディレクトリ選択エラー:", error);
    }
  };

  const handleSelectOutputDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "モデル保存先ディレクトリを選択",
      });

      if (selected) {
        setConfig({ ...config, outputDir: selected as string });
      }
    } catch (error) {
      console.error("ディレクトリ選択エラー:", error);
    }
  };

  const handleStartTraining = async () => {
    if (!config.dataDir || !config.outputDir) {
      alert("学習データディレクトリとモデル保存先を指定してください。");
      return;
    }

    if (buttonLabels.length === 0) {
      alert("ボタンラベルが検出されませんでした。");
      return;
    }

    setIsTraining(true);
    setTrainingMessage("学習を開始しています...");
    setTrainingComplete(false);
    setTrainingProgress(null);

    try {
      // クラス順序を構築: [dir_1-9], [ユーザーボタン], [others]
      const directionKeys = ["dir_1", "dir_2", "dir_3", "dir_4", "dir_6", "dir_7", "dir_8", "dir_9"];
      const orderedLabels = [...directionKeys, ...buttonLabels, "others"];

      // データディレクトリ名を取得
      const dataDirName = await path.basename(config.dataDir);
      
      // 出力ファイルパス: [保存先ディレクトリ]/[データディレクトリ名].tar.gz
      const outputFileName = `${dataDirName}.tar.gz`;
      const outputPath = await path.join(config.outputDir, outputFileName);

      // 進捗チャンネルを作成
      const progressChannel = new Channel<TrainingProgress>();
      progressChannel.onmessage = (progress) => {
        setTrainingProgress(progress);
        setTrainingMessage(progress.message);
      };

      const result = await invoke<string>("train_classification_model", {
        dataDir: config.dataDir,
        outputPath: outputPath,
        numEpochs: config.numEpochs,
        batchSize: config.batchSize,
        learningRate: config.learningRate,
        buttonLabels: orderedLabels,
        useGpu: mlBackend === "wgpu",
        onProgress: progressChannel,
      });

      setTrainingMessage(result);
      setTrainingComplete(true);
    } catch (error) {
      console.error("学習エラー:", error);
      setTrainingMessage(`エラー: ${error}`);
    } finally {
      setIsTraining(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="training-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>🧠 モデル学習</h2>
          <button className="close-button" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="modal-body">
          <div className="backend-info">
            <strong>使用バックエンド:</strong> {mlBackend.toUpperCase()}
          </div>

          <div className="config-section">
            <div className="config-item">
              <label>学習データディレクトリ</label>
              <div className="input-with-button">
                <input
                  type="text"
                  value={config.dataDir}
                  readOnly
                  placeholder="ディレクトリを選択してください"
                  disabled={isTraining}
                />
                <button onClick={handleSelectDataDir} disabled={isTraining}>
                  参照
                </button>
              </div>
            </div>

            <div className="config-item">
              <label>モデル保存先ディレクトリ</label>
              <div className="input-with-button">
                <input
                  type="text"
                  value={config.outputDir}
                  readOnly
                  placeholder="保存先ディレクトリを選択してください"
                  disabled={isTraining}
                />
                <button onClick={handleSelectOutputDir} disabled={isTraining}>
                  参照
                </button>
              </div>
              {config.dataDir && config.outputDir && (
                <div className="output-filename-hint">
                  保存ファイル名: {config.dataDir.split(/[/\\]/).pop()}.tar.gz
                </div>
              )}
            </div>

            <div className="config-row">
              <div className="config-item">
                <label>エポック数</label>
                <input
                  type="number"
                  value={config.numEpochs}
                  onChange={(e) => setConfig({ ...config, numEpochs: parseInt(e.target.value) || 50 })}
                  min="1"
                  max="1000"
                  disabled={isTraining}
                />
              </div>

              <div className="config-item">
                <label>バッチサイズ</label>
                <input
                  type="number"
                  value={config.batchSize}
                  onChange={(e) => setConfig({ ...config, batchSize: parseInt(e.target.value) || 32 })}
                  min="1"
                  max="256"
                  disabled={isTraining}
                />
              </div>

              <div className="config-item">
                <label>学習率</label>
                <input
                  type="number"
                  step="0.0001"
                  value={config.learningRate}
                  onChange={(e) => setConfig({ ...config, learningRate: parseFloat(e.target.value) || 0.001 })}
                  min="0.0001"
                  max="1"
                  disabled={isTraining}
                />
              </div>
            </div>

            {buttonLabels.length > 0 && (
              <div className="config-item">
                <label>
                  ボタンラベル ({buttonLabels.length}個)
                  <span className="label-hint">※ ドラッグ&ドロップで並び替え可能</span>
                </label>
                <div className="button-labels-sortable">
                  {buttonLabels.map((label, idx) => (
                    <div
                      key={idx}
                      className={`label-chip-draggable ${draggedIndex === idx ? "dragging" : ""}`}
                      draggable={!isTraining}
                      onDragStart={(e) => handleDragStart(e, idx)}
                      onDragOver={(e) => handleDragOver(e, idx)}
                      onDrop={(e) => handleDrop(e, idx)}
                      onDragEnd={handleDragEnd}
                    >
                      <span className="drag-handle">⋮⋮</span>
                      <span className="label-text">{label}</span>
                    </div>
                  ))}
                </div>
                <div className="class-order-info">
                  最終的なクラス順序: [dir_1～9] → [上記のボタン] → [others]
                </div>
              </div>
            )}
          </div>

          {isTraining && trainingProgress && (
            <div className="training-progress">
              <div className="progress-header">
                <strong>Epoch {trainingProgress.current_epoch}/{trainingProgress.total_epochs}</strong>
              </div>
              <div className="progress-metrics">
                <div className="metric">
                  <span className="metric-label">Train Loss:</span>
                  <span className="metric-value">{trainingProgress.train_loss.toFixed(4)}</span>
                </div>
                <div className="metric">
                  <span className="metric-label">Train Accuracy:</span>
                  <span className="metric-value">{(trainingProgress.train_accuracy * 100).toFixed(2)}%</span>
                </div>
                <div className="metric">
                  <span className="metric-label">Val Loss:</span>
                  <span className="metric-value">{trainingProgress.val_loss.toFixed(4)}</span>
                </div>
                <div className="metric">
                  <span className="metric-label">Val Accuracy:</span>
                  <span className="metric-value">{(trainingProgress.val_accuracy * 100).toFixed(2)}%</span>
                </div>
              </div>
              <div className="progress-message">{trainingProgress.message}</div>
              {trainingProgress.log_lines.length > 0 && (
                <div className="training-logs">
                  <div className="logs-header">学習ログ:</div>
                  <div className="logs-content">
                    {trainingProgress.log_lines.slice(-20).map((line, idx) => (
                      <div key={idx} className="log-line">{line}</div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {trainingMessage && !isTraining && (
            <div className={`training-message ${trainingComplete ? "complete" : "error"}`}>
              <pre>{trainingMessage}</pre>
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button className="btn-cancel" onClick={onClose} disabled={isTraining}>
            {trainingComplete ? "閉じる" : "キャンセル"}
          </button>
          <button
            className="btn-train"
            onClick={handleStartTraining}
            disabled={isTraining || !config.dataDir || !config.outputDir}
          >
            {isTraining ? "学習中..." : "学習開始"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default TrainingDialog;
