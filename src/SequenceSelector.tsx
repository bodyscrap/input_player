import { useState } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import "./SequenceSelector.css";

interface ExtractionProgress {
  current_frame: number;
  total_frames: number;
  message: string;
}

interface SequenceSelectorProps {
  onClose: () => void;
  onSelect: (
    csvPath: string,
    targetSlot: number,
    isCompatible: boolean,
  ) => void;
  availableButtons: string[];
  targetSlot: number | null;
  classificationModelPath: string | null; // MP4→CSV変換用モデル
  mlBackend: "cpu" | "wgpu";
}

function SequenceSelector({
  onClose,
  onSelect,
  availableButtons,
  targetSlot,
  classificationModelPath,
  mlBackend,
}: SequenceSelectorProps) {
  const [csvPath, setCsvPath] = useState("");
  const [selectedSlot] = useState(targetSlot ?? 0);
  const [csvButtons, setCsvButtons] = useState<string[]>([]);
  const [isCompatible, setIsCompatible] = useState(false);
  // @ts-expect-error - message is used for logging via setMessage
  const [message, setMessage] = useState("");
  const [isConverting, setIsConverting] = useState(false);
  const [progress, setProgress] = useState({ current: 0, total: 0 });

  const handleLoad = () => {
    if (csvPath) {
      onSelect(csvPath, selectedSlot, isCompatible);
      onClose();
    }
  };

  return (
    <div className="sequence-selector" onClick={onClose}>
      <div className="selector-window" onClick={(e) => e.stopPropagation()}>
        <div className="selector-header">
          <h2>シーケンス選択</h2>
          <button onClick={onClose} className="close-button">
            ✕
          </button>
        </div>

        <div className="selector-content">
          <div className="slot-selection">
            <span>スロット {selectedSlot + 1}</span>
          </div>

          <div className="path-section">
            <label>
              シーケンスファイル:
              <input
                type="text"
                value={csvPath}
                onChange={(e) => setCsvPath(e.target.value)}
                className="path-input"
              />
              <button
                onClick={async () => {
                  const file = await open({
                    multiple: false,
                    directory: false,
                    filters: [
                      { name: "Input Files", extensions: ["csv", "mp4"] },
                      { name: "CSV", extensions: ["csv"] },
                      { name: "MP4", extensions: ["mp4"] },
                    ],
                  });
                  if (file) {
                    const isMP4 = file.toLowerCase().endsWith(".mp4");
                    
                    if (isMP4) {
                      // MP4ファイルの場合
                      if (!classificationModelPath) {
                        setMessage("MP4ファイルを読み込むには、先にモデル設定が必要です");
                        setIsCompatible(false);
                        return;
                      }
                      
                      // MP4からCSVに変換
                      setIsConverting(true);
                      setCsvPath("");
                      setCsvButtons([]);
                      setProgress({ current: 0, total: 0 });
                      setMessage("MP4を解析中...");
                      
                      try {
                        console.log("[MP4変換] 開始:", file);
                        
                        // 進捗を受け取るChannelを作成
                        const onProgress = new Channel<ExtractionProgress>();
                        onProgress.onmessage = (progressData) => {
                          setProgress({
                            current: progressData.current_frame,
                            total: progressData.total_frames,
                          });
                          setMessage(progressData.message);
                        };
                        
                        const generatedCsvPath = await invoke<string>("mp4_to_sequence", {
                          videoPath: file,
                          modelPath: classificationModelPath,
                          backend: mlBackend,
                          onProgress,
                        });
                        
                        console.log("[MP4変換] 完了:", generatedCsvPath);
                        setCsvPath(generatedCsvPath);
                        
                        // 生成されたCSVの互換性チェック
                        const buttons = await api.getCsvButtonNames(generatedCsvPath);
                        setCsvButtons(buttons);
                        const unmappedButtons = buttons.filter(
                          (btn) => !availableButtons.includes(btn),
                        );
                        
                        if (unmappedButtons.length === 0 && buttons.length > 0) {
                          setIsCompatible(true);
                          setMessage(
                            `✓ シーケンスを生成しました (${buttons.length}個のボタン)`,
                          );
                          
                          // CSVと同じ形式で読み込み - 自動的にonSelectを呼ぶ
                          console.log("[MP4変換] 自動読み込み開始");
                          onSelect(generatedCsvPath, selectedSlot, true);
                          onClose();
                        } else {
                          setIsCompatible(false);
                          setMessage(
                            `✗ シーケンス用に設定されていないボタン: ${unmappedButtons.join(", ")}`,
                          );
                        }
                      } catch (error) {
                        console.error("[MP4変換] エラー:", error);
                        setMessage(`エラー: ${error}`);
                        setIsCompatible(false);
                        setCsvPath("");
                        setCsvButtons([]);
                      } finally {
                        setIsConverting(false);
                      }
                    } else {
                      // CSVファイルの場合（既存の処理）
                      setCsvPath(file);
                      // ファイル選択後に自動で互換性チェック（sequenceButtonsのみ）
                      try {
                        const buttons = await api.getCsvButtonNames(file);
                        setCsvButtons(buttons);
                        // CSVに含まれるボタンのうち、シーケンス用ボタンに含まれないものをチェック
                        const unmappedButtons = buttons.filter(
                          (btn) => !availableButtons.includes(btn),
                        );
                        if (unmappedButtons.length === 0 && buttons.length > 0) {
                          setIsCompatible(true);
                          setMessage(
                            `✓ このシーケンスは再生可能です (${buttons.length}個のボタン)`,
                          );
                        } else if (buttons.length === 0) {
                          setIsCompatible(false);
                          setMessage("ボタンが見つかりませんでした");
                        } else {
                          setIsCompatible(false);
                          setMessage(
                            `✗ シーケンス用に設定されていないボタン: ${unmappedButtons.join(", ")}`,
                          );
                        }
                      } catch (error) {
                        setIsCompatible(false);
                        const errorMessage = error instanceof Error ? error.message : String(error);
                        setMessage(`ファイル読み込みエラー: ${errorMessage}`);
                        setCsvButtons([]);
                        console.error("CSV読み込みエラー:", error);
                      }
                    }
                  }
                }}
                className="browse-button"
                disabled={isConverting}
              >
                📁
              </button>
            </label>
          </div>

          {isConverting && progress.total > 0 && (
            <div className="progress-container">
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{
                    width: `${(progress.current / progress.total) * 100}%`,
                  }}
                />
              </div>
              <div className="progress-text">
                {progress.current} / {progress.total} フレーム (
                {Math.round((progress.current / progress.total) * 100)}%)
              </div>
            </div>
          )}

          {csvButtons.length > 0 && (
            <div className="button-list">
              <div className="button-list-header">
                <h4>CSVのボタン一覧:</h4>
                <span
                  className={`compatibility-status ${isCompatible ? "status-ok" : "status-error"}`}
                >
                  {isCompatible ? "✓ 再生可能" : "✗ マッピングの修正が必要です"}
                </span>
              </div>
              <div className="button-tags">
                {csvButtons.map((btn) => {
                  const isMapped = availableButtons.includes(btn);
                  return (
                    <span
                      key={btn}
                      className={`button-tag ${isMapped ? "mapped" : "unmapped"}`}
                    >
                      {btn} {isMapped ? "✓" : "✗"}
                    </span>
                  );
                })}
              </div>
            </div>
          )}

          <div className="mapping-info">
            <h4>現在のマッピング:</h4>
            {availableButtons.length > 0 ? (
              <div className="button-tags">
                {availableButtons.map((btn) => (
                  <span key={btn} className="button-tag mapped">
                    {btn}
                  </span>
                ))}
              </div>
            ) : (
              <p>マッピングが設定されていません</p>
            )}
          </div>
        </div>

        <div className="selector-footer">
          <button
            onClick={handleLoad}
            disabled={!csvPath}
            className="load-button"
          >
            読み込み
          </button>
          <button onClick={onClose} className="cancel-button">
            キャンセル
          </button>
        </div>
      </div>
    </div>
  );
}

export default SequenceSelector;
