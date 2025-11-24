import { useState, useEffect, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type { ButtonMapping } from "./types";
import "./ButtonMappingEditor.css";

interface ButtonMappingEditorProps {
  onClose: () => void;
  initialConnected: boolean;
  isExpanded: boolean;
}

function ButtonMappingEditor({ initialConnected, isExpanded }: ButtonMappingEditorProps) {
  const [mapping, setMapping] = useState<ButtonMapping>({
    xbox: {},
    dualshock4: {},
  });
  const [message, setMessage] = useState("");
  const [isConnected, setIsConnected] = useState(initialConnected);
  const [activeTestButton, setActiveTestButton] = useState<string | null>(null);
  const [activeXboxButton, setActiveXboxButton] = useState<string | null>(null);
  const activeXboxButtonRef = useRef<string | null>(null);

  // Xbox 360コントローラーのボタン一覧
  const xboxButtons = [
    "button1", "button2", "button3", "button4",
    "button5", "button6", "button7", "button8",
    "button9", "button10"
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
  };

  // アクティブなボタンをRefで追跡
  useEffect(() => {
    activeXboxButtonRef.current = activeXboxButton;
  }, [activeXboxButton]);

  // 接続状態を同期
  useEffect(() => {
    setIsConnected(initialConnected);
  }, [initialConnected]);

  // マッピングファイルを読み込む
  useEffect(() => {
    loadMappingFromFile();
  }, []);

  // ボタンを押している間、継続的に入力を送信
  useEffect(() => {
    if (!isConnected || !isExpanded) return;

    const interval = setInterval(async () => {
      const currentButton = activeXboxButtonRef.current;
      if (!currentButton) return;

      const buttons: Record<string, number> = {
        [currentButton]: 1
      };
      
      try {
        await api.updateManualInput(5, buttons);
      } catch (error) {
        console.error("Test button error:", error);
      }
    }, 10);

    return () => clearInterval(interval);
  }, [isConnected, isExpanded]);

  const loadMappingFromFile = async () => {
    try {
      const loaded = await api.loadButtonMapping("config/button_mapping.json");
      setMapping(loaded);
      setMessage("マッピングを読み込みました");
    } catch (error) {
      setMessage("設定ファイルが見つかりません。CSVファイルを選択してください。");
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
      const newMapping: ButtonMapping = {
        xbox: {},
        dualshock4: {},
      };
      
      buttons.forEach((btnName, index) => {
        const xboxBtn = `button${Math.min(index + 1, 10)}`;
        newMapping.xbox[btnName] = xboxBtn;
      });
      
      setMapping(newMapping);
      setMessage(`CSVから${buttons.length}個のボタンを検出し、マッピングを作成しました`);
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
      return {
        ...prev,
        xbox: newXbox,
      };
    });
  };

  const addNewMapping = () => {
    const csvButton = prompt("CSVのボタン名を入力してください（例: punch, kick, jump）");
    if (csvButton && csvButton.trim()) {
      handleXboxMappingChange("button1", csvButton.trim());
    }
  };

  const handleTestButtonPress = (csvButton: string, xboxButton: string) => {
    if (!isConnected) return;
    
    setActiveTestButton(csvButton);
    setActiveXboxButton(xboxButton);
  };

  const handleTestButtonRelease = () => {
    setActiveTestButton(null);
    setActiveXboxButton(null);
  };

  const csvToXbox = mapping.xbox;
  const csvButtons = Object.keys(csvToXbox);

  if (!isExpanded) return null;

  return (
    <div className="button-mapping-editor-inline">
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
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {csvButtons.length === 0 ? (
            <tr>
              <td colSpan={4} className="empty-message">
                マッピングがありません。CSVファイルを選択するか、「+ 追加」をクリックしてください。
              </td>
            </tr>
          ) : (
            csvButtons.map((csvButton) => (
              <tr key={csvButton}>
                <td>
                  <button
                    className={`test-csv-button-inline ${activeTestButton === csvButton ? 'active' : ''}`}
                    onMouseDown={() => handleTestButtonPress(csvButton, csvToXbox[csvButton])}
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
                    value={csvToXbox[csvButton]}
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
                <td>
                  <button
                    onClick={() => removeXboxMapping(csvButton)}
                    className="remove-button-inline"
                  >
                    削除
                  </button>
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

export default ButtonMappingEditor;
