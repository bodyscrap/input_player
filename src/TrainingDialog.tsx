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
    outputDir: "", // useEffectで初期化
    numEpochs: 50,
    batchSize: 8,  // input_analyzerと同じデフォルト値（GPUメモリ効率的）
    learningRate: 0.001,
  });

  const [buttonLabels, setButtonLabels] = useState<string[]>([]);
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  const [isTraining, setIsTraining] = useState(false);
  const [trainingProgress, setTrainingProgress] = useState<TrainingProgress | null>(null);
  const [trainingMessage, setTrainingMessage] = useState<string>("");
  const [trainingComplete, setTrainingComplete] = useState(false);

  // 初回起動時にmodelsディレクトリをデフォルト出力先に設定
  useEffect(() => {
    const initOutputDir = async () => {
      const savedOutputDir = localStorage.getItem("trainingOutputDir");
      if (savedOutputDir) {
        setConfig((prev) => ({ ...prev, outputDir: savedOutputDir }));
      } else {
        // デフォルト: 実行ファイル(exe)と同階層のmodelsディレクトリ
        const appDir = await invoke<string>("get_app_dir");
        const defaultModelsDir = await path.join(appDir, "models");
        setConfig((prev) => ({ ...prev, outputDir: defaultModelsDir }));
      }
    };
    initOutputDir();
  }, []);

  // ボタンラベルを自動検出（メタデータ優先）
  useEffect(() => {
    if (config.dataDir) {
      loadButtonLabelsWithMetadata();
    }
  }, [config.dataDir]);

  const loadButtonLabelsWithMetadata = async () => {
    try {
      // 1. メタデータファイルがあれば読み込み
      const metadata = await invoke<string[] | null>("load_button_order_metadata", {
        dataDir: config.dataDir,
      });
      
      if (metadata && metadata.length > 0) {
        console.log("[メタデータ] ボタン順序を復元:", metadata);
        setButtonLabels(metadata);
      } else {
        // 2. メタデータがない場合はフォルダ名から生成して保存
        console.log("[メタデータ] メタデータがないため、ボタンを自動検出して保存");
        const labels = await invoke<string[]>("get_button_labels_from_data_dir", {
          dataDir: config.dataDir,
        });
        setButtonLabels(labels);
        
        // 検出したラベルをメタデータとして保存
        if (labels.length > 0) {
          await invoke("save_button_order_metadata", {
            dataDir: config.dataDir,
            buttonLabels: labels,
          });
          console.log("[メタデータ] 初期ボタン順序を保存:", labels);
        }
      }
    } catch (error) {
      console.error("ボタンラベルの検出/読み込みに失敗:", error);
      setButtonLabels([]);
    }
  };

  // マウスベースのドラッグ&ドロップでボタンラベルを並び替え
  const handleMouseDown = (index: number) => {
    if (isTraining) return;
    setDraggedIndex(index);
  };

  const handleMouseEnter = (index: number) => {
    if (draggedIndex === null || isTraining) return;
    setDragOverIndex(index);
  };

  const handleMouseUp = async () => {
    if (draggedIndex === null || dragOverIndex === null || isTraining) {
      setDraggedIndex(null);
      setDragOverIndex(null);
      return;
    }

    if (draggedIndex === dragOverIndex) {
      setDraggedIndex(null);
      setDragOverIndex(null);
      return;
    }

    const newLabels = [...buttonLabels];
    const [draggedItem] = newLabels.splice(draggedIndex, 1);
    newLabels.splice(dragOverIndex, 0, draggedItem);

    setButtonLabels(newLabels);
    setDraggedIndex(null);
    setDragOverIndex(null);

    // 3. 並び替え時にメタデータを更新
    try {
      await invoke("save_button_order_metadata", {
        dataDir: config.dataDir,
        buttonLabels: newLabels,
      });
      console.log("[メタデータ] 並び替え後の順序を保存:", newLabels);
    } catch (error) {
      console.error("メタデータ保存エラー:", error);
    }
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
        // useEffectが自動的にメタデータ読み込みを実行
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
        const selectedPath = selected as string;
        setConfig({ ...config, outputDir: selectedPath });
        // 選択したパスをlocalStorageに保存
        localStorage.setItem("trainingOutputDir", selectedPath);
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

      // ボタン順序メタデータを保存
      try {
        await invoke("save_button_order_metadata", {
          dataDir: config.dataDir,
          buttonLabels: buttonLabels,
        });
        console.log("[メタデータ] ボタン順序を保存:", buttonLabels);
      } catch (error) {
        console.error("メタデータ保存エラー:", error);
        // エラーでも学習は続行
      }

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
    <div className="training-dialog-overlay" onClick={onClose}>
      <div className="training-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="training-header">
          <h2>🧠 モデル学習</h2>
          <button className="close-button" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="training-content">
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
                      className={`label-chip-draggable ${draggedIndex === idx ? "dragging" : ""} ${dragOverIndex === idx && draggedIndex !== idx ? "drag-over" : ""}`}
                      onMouseDown={() => handleMouseDown(idx)}
                      onMouseEnter={() => handleMouseEnter(idx)}
                      onMouseUp={handleMouseUp}
                      style={{ cursor: isTraining ? "default" : "grab" }}
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

        <div className="training-footer">
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
