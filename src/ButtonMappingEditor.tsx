import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type { ButtonMapping } from "./types";
import "./ButtonMappingEditor.css";

interface ButtonMappingEditorProps {
  onClose: () => void;
  initialConnected: boolean;
  activeTestButton: string | null;
  setActiveTestButton: (button: string | null) => void;
  onMappingSaved?: () => void; // マッピング保存時のコールバック
}

function ButtonMappingEditor({ onClose, initialConnected, activeTestButton, setActiveTestButton, onMappingSaved }: ButtonMappingEditorProps) {
  const [mapping, setMapping] = useState<ButtonMapping>({
    xbox: {},
    dualshock4: {},
    sequenceButtons: [],
  });
  const [message, setMessage] = useState("");
  const [isConnected, setIsConnected] = useState(initialConnected);

  // Xbox 360コントローラーのボタン一覧
  const xboxButtons = [
    "button1", "button2", "button3", "button4",
    "button5", "button6", "button7", "button8",
    "button9", "button10", "button11", "button12"
  ];

  const xboxButtonLabels: Record<string, string> = {
    "button1": "A",
    "button2": "B",
    "button3": "X",
    "button4": "Y",
    "button5": "LB",
    "button6": "RB",
    "button7": "LT",
    "button8": "RT",
    "button9": "BACK",
    "button10": "START",
    "button11": "LS",
    "button12": "RS",
  };

  // 接続状態を同期
  useEffect(() => {
    setIsConnected(initialConnected);
  }, [initialConnected]);

  // 前回のマッピングファイルを読み込む
  useEffect(() => {
    loadMappingFromFile();
  }, []);

  const loadMappingFromFile = async () => {
    try {
      const loaded = await api.loadButtonMapping("config/button_mapping.json");
      setMapping(loaded);
      setMessage("前回のマッピングを読み込みました");
    } catch (error) {
      setMessage("マッピング設定が見つかりません。ファイルを選択してください。");
      setMapping({
        xbox: {},
        dualshock4: {},
      });
    }
  };

  const saveMappingToFile = async () => {
    try {
      await api.saveButtonMapping("config/button_mapping.json", mapping);
      setMessage("マッピングを保存しました");
      // 親コンポーネントに保存を通知
      if (onMappingSaved) {
        onMappingSaved();
      }
    } catch (error) {
      setMessage(`保存エラー: ${error}`);
    }
  };

  const handleMappingFileSelect = async () => {
    const file = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'JSON', extensions: ['json'] }]
    });
    
    if (!file) return;

    try {
      const loaded = await api.loadButtonMapping(file);
      setMapping(loaded);
      setMessage("マッピングファイルを読み込みました");
      // 自動保存
      await api.saveButtonMapping("config/button_mapping.json", loaded);
    } catch (error) {
      setMessage(`読み込みエラー: ${error}`);
    }
  };

  const handleCsvFileSelect = async () => {
    const file = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'CSV', extensions: ['csv'] }]
    });
    
    if (!file) return;

    try {
      const buttons = await api.getCsvButtonNames(file);
      
      // 自動マッピング作成
      // 既存のsequenceButtonsを保持しつつ、CSVのボタンを追加
      const currentSequenceButtons = mapping.sequenceButtons || [];
      const allSequenceButtons = [...new Set([...currentSequenceButtons, ...buttons])];
      
      const newMapping: ButtonMapping = {
        ...mapping,
        sequenceButtons: allSequenceButtons,
      };
      
      buttons.forEach((btnName, index) => {
        // 既存のマッピングがなければ自動割り当て
        if (!newMapping.xbox[btnName]) {
          const xboxBtn = `button${Math.min(index + 1, 10)}`;
          newMapping.xbox[btnName] = xboxBtn;
        }
      });
      
      setMapping(newMapping);
      setMessage(`CSVから${buttons.length}個のボタンを検出し、マッピングを作成しました`);
      // 自動保存
      await api.saveButtonMapping("config/button_mapping.json", newMapping);
    } catch (error) {
      setMessage(`CSV読み込みエラー: ${error}`);
    }
  };

  const handleXboxMappingChange = (xboxButton: string, csvButton: string) => {
    setMapping((prev) => ({
      ...prev,
      xbox: {
        ...prev.xbox,
        [csvButton]: xboxButton,
      },
    }));
  };

  const removeXboxMapping = (csvButton: string) => {
    setMapping((prev) => {
      const newXbox = { ...prev.xbox };
      delete newXbox[csvButton];
      const newSequenceButtons = (prev.sequenceButtons || []).filter(btn => btn !== csvButton);
      return {
        ...prev,
        xbox: newXbox,
        sequenceButtons: newSequenceButtons,
      };
    });
  };

  const toggleSequenceButton = (csvButton: string) => {
    setMapping((prev) => {
      const currentSequenceButtons = prev.sequenceButtons || [];
      const isCurrentlyInSequence = currentSequenceButtons.includes(csvButton);
      
      const newSequenceButtons = isCurrentlyInSequence
        ? currentSequenceButtons.filter(btn => btn !== csvButton)
        : [...currentSequenceButtons, csvButton];
      
      return {
        ...prev,
        sequenceButtons: newSequenceButtons,
      };
    });
  };

  const addNewMapping = () => {
    const csvButton = prompt("CSVのボタン名を入力してください（例: punch, kick, jump）");
    if (csvButton && csvButton.trim()) {
      handleXboxMappingChange("button1", csvButton.trim());
    }
  };

  const handleTestButtonPress = (_csvButton: string, xboxButton: string) => {
    if (!isConnected) return;
    setActiveTestButton(xboxButton); // Xboxボタン名を設定
  };

  const handleTestButtonRelease = () => {
    setActiveTestButton(null);
  };

  const csvToXbox = mapping.xbox;
  const csvButtons = Object.keys(csvToXbox);

  return (
    <div className="button-mapping-editor-overlay" onClick={onClose}>
      <div className="button-mapping-editor-window" onClick={(e) => e.stopPropagation()}>
        <div className="editor-header">
          <h2>ボタンマッピング設定</h2>
          <button onClick={onClose} className="close-button">
            ✕
          </button>
        </div>

        <div className="editor-content">
          <div className="editor-controls">
            <button onClick={handleMappingFileSelect} className="btn-file">
              📁 マッピング設定ファイルを開く
            </button>
            <button onClick={handleCsvFileSelect} className="btn-file">
              📄 入力履歴CSVから作成
            </button>
            <button onClick={saveMappingToFile} className="btn-save">
              💾 保存
            </button>
            <button onClick={addNewMapping} className="btn-add">
              + 追加
            </button>
          </div>

          {message && <div className="message-inline">{message}</div>}

          {!isConnected && (
            <div className="warning-message">
              ⚠️ コントローラーが未接続です。ボタンテストを行うには接続してください。
            </div>
          )}

          {isConnected && (
            <div className="info-message">
              💡 CSVボタン名をクリックして動作を確認できます。
            </div>
          )}

          <table className="mapping-table-inline">
        <thead>
          <tr>
            <th>CSVボタン名</th>
            <th>→</th>
            <th>Xboxボタン</th>
            <th>シーケンス</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {csvButtons.length === 0 ? (
            <tr>
              <td colSpan={5} className="empty-message">
                マッピングがありません。CSVファイルを選択するか、「+ 追加」をクリックしてください。
              </td>
            </tr>
          ) : (
            csvButtons.map((csvButton) => {
              const xboxButton = csvToXbox[csvButton];
              const isActive = activeTestButton === xboxButton;
              
              return (
                <tr key={csvButton}>
                  <td>
                    <button
                      className={`test-csv-button-inline ${isActive ? 'active' : ''}`}
                      onMouseDown={() => handleTestButtonPress(csvButton, xboxButton)}
                      onMouseUp={handleTestButtonRelease}
                      onMouseLeave={handleTestButtonRelease}
                      disabled={!isConnected}
                    >
                      {csvButton}
                    </button>
                  </td>
                  <td>→</td>
                  <td>
                    <select
                      value={xboxButton}
                      onChange={(e) => handleXboxMappingChange(e.target.value, csvButton)}
                      className="xbox-button-select-inline"
                    >
                      {xboxButtons.map((btn) => (
                        <option key={btn} value={btn}>
                          {xboxButtonLabels[btn]} ({btn})
                        </option>
                      ))}
                    </select>
                  </td>
                  <td className="checkbox-cell">
                    <input
                      type="checkbox"
                      checked={mapping.sequenceButtons?.includes(csvButton) ?? false}
                      onChange={() => toggleSequenceButton(csvButton)}
                    />
                  </td>
                  <td>
                    <button
                      onClick={() => removeXboxMapping(csvButton)}
                      className="remove-button-inline"
                    >
                      削除
                    </button>
                  </td>
                </tr>
              );
            })
          )}
        </tbody>
      </table>
        </div>
      </div>
    </div>
  );
}

export default ButtonMappingEditor;
