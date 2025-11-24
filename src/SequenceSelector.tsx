import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import "./SequenceSelector.css";

interface SequenceSelectorProps {
  onClose: () => void;
  onSelect: (csvPath: string, targetSlot: number) => void;
  availableButtons: string[];
  targetSlot: number | null;
  currentSlots: (string | null)[];
}

function SequenceSelector({ onClose, onSelect, availableButtons, targetSlot, currentSlots }: SequenceSelectorProps) {
  const [csvPath, setCsvPath] = useState("");
  const [selectedSlot, setSelectedSlot] = useState(targetSlot ?? 0);
  const [csvButtons, setCsvButtons] = useState<string[]>([]);
  const [isCompatible, setIsCompatible] = useState(false);
  const [message, setMessage] = useState("");

  const checkCompatibility = async () => {
    if (!csvPath) {
      setMessage("ファイルを選択してください");
      return;
    }
    try {
      const buttons = await api.getCsvButtonNames(csvPath);
      setCsvButtons(buttons);
      
      // CSVのボタンがすべてマッピング済みかチェック
      const unmappedButtons = buttons.filter(btn => !availableButtons.includes(btn));
      
      if (unmappedButtons.length === 0 && buttons.length > 0) {
        setIsCompatible(true);
        setMessage(`✓ このシーケンスは再生可能です (${buttons.length}個のボタン)`);
      } else if (buttons.length === 0) {
        setIsCompatible(false);
        setMessage("ボタンが見つかりませんでした");
      } else {
        setIsCompatible(false);
        setMessage(`✗ マッピングされていないボタン: ${unmappedButtons.join(", ")}`);
      }
    } catch (error) {
      setIsCompatible(false);
      setMessage(`エラー: ${error}`);
      setCsvButtons([]);
    }
  };

  const handleLoad = () => {
    if (isCompatible && csvPath) {
      onSelect(csvPath, selectedSlot);
      onClose();
    }
  };

  return (
    <div className="sequence-selector" onClick={onClose}>
      <div className="selector-window" onClick={(e) => e.stopPropagation()}>
        <div className="selector-header">
          <h2>シーケンス選択</h2>
          <button onClick={onClose} className="close-button">✕</button>
        </div>

        <div className="selector-content">
          <div className="slot-selection">
            <h4>ロード先スロット:</h4>
            <div className="slot-buttons">
              {[0, 1, 2, 3].map((i) => (
                <button
                  key={i}
                  onClick={() => setSelectedSlot(i)}
                  className={`slot-btn ${selectedSlot === i ? 'selected' : ''} ${currentSlots[i] ? 'occupied' : 'empty'}`}
                  title={currentSlots[i] || `スロット${i + 1}`}
                >
                  {i + 1} {currentSlots[i] && '📄'}
                </button>
              ))}
            </div>
          </div>
          
          <div className="path-section">
            <label>
              入力履歴CSVファイル:
              <input
                type="text"
                value={csvPath}
                onChange={(e) => setCsvPath(e.target.value)}
                className="path-input"
              />
              <button onClick={async () => {
                const file = await open({
                  multiple: false,
                  directory: false,
                  filters: [{ name: 'CSV', extensions: ['csv'] }]
                });
                if (file) {
                  setCsvPath(file);
                  // ファイル選択後に自動で互換性チェック
                  try {
                    const buttons = await api.getCsvButtonNames(file);
                    setCsvButtons(buttons);
                    const unmappedButtons = buttons.filter(btn => !availableButtons.includes(btn));
                    if (unmappedButtons.length === 0 && buttons.length > 0) {
                      setIsCompatible(true);
                      setMessage(`✓ このシーケンスは再生可能です (${buttons.length}個のボタン)`);
                    } else if (buttons.length === 0) {
                      setIsCompatible(false);
                      setMessage("ボタンが見つかりませんでした");
                    } else {
                      setIsCompatible(false);
                      setMessage(`✗ マッピングされていないボタン: ${unmappedButtons.join(", ")}`);
                    }
                  } catch (error) {
                    setIsCompatible(false);
                    setMessage(`エラー: ${error}`);
                    setCsvButtons([]);
                  }
                }
              }} className="browse-button">📁</button>
            </label>
            <button onClick={checkCompatibility}>互換性チェック</button>
          </div>

          {message && (
            <div className={`compatibility-message ${isCompatible ? 'compatible' : 'incompatible'}`}>
              {message}
            </div>
          )}

          {csvButtons.length > 0 && (
            <div className="button-list">
              <h4>CSVのボタン一覧:</h4>
              <div className="button-tags">
                {csvButtons.map((btn) => {
                  const isMapped = availableButtons.includes(btn);
                  return (
                    <span key={btn} className={`button-tag ${isMapped ? 'mapped' : 'unmapped'}`}>
                      {btn} {isMapped ? '✓' : '✗'}
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
          <button onClick={handleLoad} disabled={!isCompatible} className="load-button">
            読み込み
          </button>
          <button onClick={onClose} className="cancel-button">キャンセル</button>
        </div>
      </div>
    </div>
  );
}

export default SequenceSelector;
