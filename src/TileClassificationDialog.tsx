import { useState, useEffect } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { path } from "@tauri-apps/api";
import "./TrainingDialog.css"; // 同じスタイルを使用

interface TileClassificationDialogProps {
  mlBackend: "cpu" | "wgpu";
  onClose: () => void;
}

interface ClassificationConfig {
  modelPath: string;
  videoPath: string;
  outputDir: string;
  frameSkip: number;
}

interface ModelMetadata {
  button_labels: string[];
  all_class_labels: string[]; // 全クラス名（方向8個 + ボタン + others）
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

interface ExtractionProgress {
  current_frame: number;
  total_frames: number;
  message: string;
}

interface ClassSummary {
  class_name: string;
  count: number;
}

interface ClassificationResult {
  summary: ClassSummary[];
  message: string;
}

function TileClassificationDialog({ mlBackend, onClose }: TileClassificationDialogProps) {
  const [config, setConfig] = useState<ClassificationConfig>({
    modelPath: "",
    videoPath: "",
    outputDir: "",
    frameSkip: 0,
  });

  const [metadata, setMetadata] = useState<ModelMetadata | null>(null);
  const [isClassifying, setIsClassifying] = useState(false);
  const [progress, setProgress] = useState<ExtractionProgress | null>(null);
  const [result, setResult] = useState<ClassificationResult | null>(null);
  const [errorMessage, setErrorMessage] = useState<string>("");

  // 初回起動時の出力先設定
  useEffect(() => {
    const initOutputDir = async () => {
      const savedOutputDir = localStorage.getItem("classificationOutputDir");
      if (savedOutputDir) {
        setConfig((prev) => ({ ...prev, outputDir: savedOutputDir }));
      } else {
        // デフォルト: 実行ファイルと同階層のclassified_tilesディレクトリ
        const appDir = await invoke<string>("get_app_dir");
        const defaultOutputDir = await path.join(appDir, "classified_tiles");
        setConfig((prev) => ({ ...prev, outputDir: defaultOutputDir }));
      }
    };
    initOutputDir();
  }, []);

  // モデル選択
  const handleSelectModel = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Model", extensions: ["tar.gz", "gz"] }],
        title: "分類モデルを選択",
      });

      if (selected) {
        const modelPath = selected as string;
        console.log("選択したモデルパス:", modelPath);
        setConfig({ ...config, modelPath });

        // メタデータ読み込み
        try {
          console.log("メタデータ読み込み開始...");
          const meta = await invoke<ModelMetadata>("get_model_metadata", {
            modelPath,
          });
          console.log("メタデータ読み込み成功:", JSON.stringify(meta, null, 2));
          console.log("button_labels:", meta.button_labels);
          console.log("all_class_labels:", meta.all_class_labels);
          setMetadata(meta);
          setErrorMessage("");
          
          // ボタンラベル情報をApp.tsxに渡してマッピングを設定
          if (meta.button_labels && meta.button_labels.length > 0) {
            await applyButtonMappingFromModel(meta.button_labels);
          }
        } catch (error) {
          console.error("メタデータ読み込みエラー:", error);
          setErrorMessage(`メタデータ読み込みエラー: ${error}`);
          setMetadata(null);
        }
      }
    } catch (error) {
      console.error("モデル選択エラー:", error);
    }
  };

  // モデルのボタンラベルからマッピングを生成
  const applyButtonMappingFromModel = async (buttonLabels: string[]) => {
    try {
      console.log("[ボタンマッピング] モデルから設定:", buttonLabels);
      
      // デフォルトマッピング: ボタン名 -> ボタン名
      const mapping: Record<string, string> = {};
      buttonLabels.forEach((label) => {
        mapping[label] = label;
      });
      
      // ボタンマッピングファイルを保存
      await invoke("save_button_mapping", {
        mapping,
        filePath: "config/button_mapping.json",
      });
      
      console.log("[ボタンマッピング] 設定完了:", mapping);
    } catch (error) {
      console.error("ボタンマッピング設定エラー:", error);
    }
  };

  // 動画選択
  const handleSelectVideo = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Video", extensions: ["mp4", "avi", "mkv", "mov", "webm"] },
        ],
        title: "分類対象の動画を選択",
      });

      if (selected) {
        const videoPath = selected as string;

        // 動画サイズチェック
        if (metadata) {
          try {
            const videoInfo = await invoke<{ width: number; height: number }>(
              "get_video_info",
              { videoPath }
            );

            if (
              videoInfo.width !== metadata.video_width ||
              videoInfo.height !== metadata.video_height
            ) {
              setErrorMessage(
                `動画サイズが不一致: 動画=${videoInfo.width}x${videoInfo.height}, モデル=${metadata.video_width}x${metadata.video_height}`
              );
              return;
            }

            setConfig({ ...config, videoPath });
            setErrorMessage("");
            
            // 動画名ベースの出力ディレクトリを生成
            await updateOutputDirFromVideo(videoPath);
          } catch (error) {
            console.error("動画情報取得エラー:", error);
            setErrorMessage(`動画情報取得エラー: ${error}`);
          }
        } else {
          setConfig({ ...config, videoPath });
          await updateOutputDirFromVideo(videoPath);
        }
      }
    } catch (error) {
      console.error("動画選択エラー:", error);
    }
  };

  // 動画名から出力ディレクトリを更新
  const updateOutputDirFromVideo = async (videoPath: string) => {
    try {
      const savedOutputDir = localStorage.getItem("classificationOutputDir");
      const baseDir = savedOutputDir || await invoke<string>("get_app_dir");
      
      // 動画ファイル名（拡張子なし）を取得
      const videoFileName = await path.basename(videoPath);
      const videoBaseName = videoFileName.replace(/\.[^.]+$/, ""); // 拡張子を削除
      
      // baseDir直下に動画名のディレクトリが作成される
      // （バックエンドが自動的に作成するので、ここでは親ディレクトリのみ設定）
      setConfig((prev) => ({ ...prev, outputDir: baseDir }));
      console.log("出力先ディレクトリ（親）を設定:", baseDir);
      console.log("実際の出力先:", `${baseDir}/${videoBaseName}/`);
    } catch (error) {
      console.error("出力ディレクトリ設定エラー:", error);
    }
  };

  // 出力先選択
  const handleSelectOutputDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "分類結果の保存先を選択",
      });

      if (selected) {
        const selectedPath = selected as string;
        setConfig({ ...config, outputDir: selectedPath });
        localStorage.setItem("classificationOutputDir", selectedPath);
      }
    } catch (error) {
      console.error("ディレクトリ選択エラー:", error);
    }
  };

  // 分類実行
  const handleStartClassification = async () => {
    if (!config.modelPath || !config.videoPath || !config.outputDir) {
      setErrorMessage("モデル、動画、出力先をすべて選択してください");
      return;
    }

    if (!metadata) {
      setErrorMessage("モデルのメタデータが読み込まれていません");
      return;
    }

    setIsClassifying(true);
    setResult(null);
    setErrorMessage("");

    try {
      // 進捗チャンネル作成
      const progressChannel = new Channel<ExtractionProgress>();
      progressChannel.onmessage = (prog) => {
        setProgress(prog);
      };

      const classificationResult = await invoke<ClassificationResult>(
        "extract_and_classify_tiles",
        {
          videoPath: config.videoPath,
          modelPath: config.modelPath,
          outputDir: config.outputDir,
          frameSkip: config.frameSkip,
          useGpu: mlBackend === "wgpu",
          onProgress: progressChannel,
        }
      );

      setResult(classificationResult);
      setProgress(null);
    } catch (error) {
      console.error("分類エラー:", error);
      setErrorMessage(`分類エラー: ${error}`);
    } finally {
      setIsClassifying(false);
    }
  };

  return (
    <div className="modal-overlay">
      <div className="training-dialog">
        <div className="modal-header">
          <h2>タイル分類</h2>
          <button className="close-button" onClick={onClose} disabled={isClassifying}>
            ×
          </button>
        </div>

        <div className="modal-body">
          {/* モデル選択 */}
          <div className="config-item">
            <label>分類モデル (*.tar.gz)</label>
            <div className="input-with-button">
              <input
                type="text"
                value={config.modelPath}
                readOnly
                placeholder="モデルファイルを選択"
              />
              <button onClick={handleSelectModel} disabled={isClassifying}>
                参照
              </button>
            </div>
          </div>

          {/* モデル情報表示 */}
          {metadata ? (
            <div className="metadata-info">
              <h4>モデル情報</h4>
              <div className="info-grid">
                <div className="info-item">
                  <span className="info-label">対応ボタン:</span>
                  <span className="info-value">{metadata.button_labels?.join(", ") || "なし"}</span>
                </div>
                {metadata.all_class_labels && metadata.all_class_labels.length > 0 && (
                  <div className="info-item">
                    <span className="info-label">全クラス:</span>
                    <span className="info-value">{metadata.all_class_labels.join(", ")}</span>
                  </div>
                )}
                <div className="info-item">
                  <span className="info-label">動画サイズ:</span>
                  <span className="info-value">{metadata.video_width} x {metadata.video_height}</span>
                </div>
                <div className="info-item">
                  <span className="info-label">タイル設定:</span>
                  <span className="info-value">{metadata.tile_width} x {metadata.tile_height} (列数: {metadata.columns_per_row})</span>
                </div>
                <div className="info-item">
                  <span className="info-label">学習日時:</span>
                  <span className="info-value">{new Date(metadata.trained_at).toLocaleString("ja-JP")}</span>
                </div>
              </div>
            </div>
          ) : (
            <div className="metadata-info">
              <p>モデルを選択してください</p>
            </div>
          )}

          {/* 動画選択 */}
          <div className="config-item">
            <label>分類対象動画</label>
            <div className="input-with-button">
              <input
                type="text"
                value={config.videoPath}
                readOnly
                placeholder="動画ファイルを選択"
              />
              <button
                onClick={handleSelectVideo}
                disabled={isClassifying || !metadata}
              >
                参照
              </button>
            </div>
          </div>

          {/* 出力先選択 */}
          <div className="config-item">
            <label>出力先ディレクトリ</label>
            <div className="input-with-button">
              <input
                type="text"
                value={config.outputDir}
                readOnly
                placeholder="出力先を選択"
              />
              <button onClick={handleSelectOutputDir} disabled={isClassifying}>
                参照
              </button>
            </div>
          </div>

          {/* 間引き設定 */}
          <div className="config-item">
            <label>
              フレーム間引き数
              <span className="hint-text">
                (0=すべて処理, 1=1フレーム毎に処理, 2=2フレーム毎...)
              </span>
            </label>
            <input
              type="number"
              value={config.frameSkip}
              onChange={(e) =>
                setConfig({ ...config, frameSkip: parseInt(e.target.value) || 0 })
              }
              min={0}
              disabled={isClassifying}
            />
          </div>

          {/* エラーメッセージ */}
          {errorMessage && (
            <div className="error-banner">{errorMessage}</div>
          )}

          {/* 進捗表示 */}
          {progress && (
            <div className="progress-container">
              <div className="progress-message">{progress.message}</div>
              <div className="progress-bar-wrapper">
                <div
                  className="progress-bar-inner"
                  style={{
                    width: progress.total_frames > 0
                      ? `${(progress.current_frame / progress.total_frames) * 100}%`
                      : '100%',
                  }}
                >
                  {progress.total_frames > 0
                    ? `${Math.round((progress.current_frame / progress.total_frames) * 100)}%`
                    : '処理中...'}
                </div>
              </div>
              <div className="progress-stats">
                {progress.current_frame} {progress.total_frames > 0 ? `/ ${progress.total_frames}` : ''} フレーム
              </div>
            </div>
          )}

          {/* 結果表示 */}
          {result && (
            <div className="results-summary">
              <h4>{result.message}</h4>
              <table className="results-table">
                <thead>
                  <tr>
                    <th>クラス名</th>
                    <th>タイル数</th>
                  </tr>
                </thead>
                <tbody>
                  {result.summary.map((item) => (
                    <tr key={item.class_name}>
                      <td>{item.class_name}</td>
                      <td>{item.count}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button
            className="primary-button"
            onClick={handleStartClassification}
            disabled={
              isClassifying ||
              !config.modelPath ||
              !config.videoPath ||
              !config.outputDir ||
              !metadata
            }
          >
            {isClassifying ? "分類中..." : "分類開始"}
          </button>
          <button onClick={onClose} disabled={isClassifying}>
            閉じる
          </button>
        </div>
      </div>
    </div>
  );
}

export default TileClassificationDialog;
