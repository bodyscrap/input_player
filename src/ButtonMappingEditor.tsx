import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import type { ButtonMapping, UserButton } from "./types";
import "./ButtonMappingEditor.css";

interface ButtonMappingEditorProps {
  onClose: () => void;
  initialConnected: boolean;
  activeTestButton: string | null;
  setActiveTestButton: (button: string | null) => void;
  onMappingSaved?: (filePath: string) => void; // マッピング保存時のコールバック（保存したファイルパスを渡す）
  currentMappingPath?: string; // 現在適用中のマッピングファイルパス
}

function ButtonMappingEditor({ onClose, initialConnected, activeTestButton, setActiveTestButton, onMappingSaved, currentMappingPath = "config/button_mapping.json" }: ButtonMappingEditorProps) {
  const [mapping, setMapping] = useState<ButtonMapping>({
    controller_type: "xbox",
    mapping: [],
  });
  const [message, setMessage] = useState("");
  const [isConnected, setIsConnected] = useState(initialConnected);
  const [currentFilePath, setCurrentFilePath] = useState<string>(currentMappingPath);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);

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

  // 現在適用中のマッピングファイルを読み込む
  useEffect(() => {
    loadMappingFromFile();
  }, []);

  const loadMappingFromFile = async () => {
    try {
      const loaded = await api.loadButtonMapping(currentMappingPath);
      setMapping(loaded);
      setCurrentFilePath(currentMappingPath);
      setHasUnsavedChanges(false);
      setMessage(`現在のマッピングを読み込みました: ${currentMappingPath}`);
    } catch (error) {
      setMessage("マッピング設定が見つかりません。ファイルを選択してください。");
      const emptyMapping: ButtonMapping = {
        controller_type: "xbox",
        mapping: [],
      };
      setMapping(emptyMapping);
      setHasUnsavedChanges(false);
    }
  };

  const saveMappingToFile = async () => {
    try {
      await api.saveButtonMapping(currentFilePath, mapping);
      setHasUnsavedChanges(false);
      setMessage(`マッピングを保存しました: ${currentFilePath}`);
      // 親コンポーネントに保存を通知（ファイルパスを渡す）
      if (onMappingSaved) {
        onMappingSaved(currentFilePath);
      }
    } catch (error) {
      setMessage(`保存エラー: ${error}`);
    }
  };

  const handleNewMapping = () => {
    if (hasUnsavedChanges) {
      const confirmed = window.confirm("保存されていない変更があります。破棄して新規作成しますか?");
      if (!confirmed) return;
    }
    const newMapping: ButtonMapping = {
      controller_type: "xbox",
      mapping: [],
    };
    setMapping(newMapping);
    setCurrentFilePath("");
    setHasUnsavedChanges(false);
    setMessage("新規マッピングを作成しました。ボタンを追加してください。");
  };

  const handleSaveAs = async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const file = await save({
      filters: [{ name: 'JSON', extensions: ['json'] }],
      defaultPath: 'button_mapping.json',
    });
    
    if (!file) return;

    try {
      await api.saveButtonMapping(file, mapping);
      setCurrentFilePath(file);
      setHasUnsavedChanges(false);
      setMessage(`マッピングを保存しました: ${file}`);
      // 親コンポーネントに保存を通知（ファイルパスを渡す）
      if (onMappingSaved) {
        onMappingSaved(file);
      }
    } catch (error) {
      setMessage(`保存エラー: ${error}`);
    }
  };

  const handleMappingFileSelect = async () => {
    if (hasUnsavedChanges) {
      const confirmed = window.confirm("保存されていない変更があります。破棄して新しいファイルを開きますか?");
      if (!confirmed) return;
    }

    const file = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'JSON', extensions: ['json'] }]
    });
    
    if (!file) return;

    try {
      const loaded = await api.loadButtonMapping(file);
      setMapping(loaded);
      setCurrentFilePath(file);
      setHasUnsavedChanges(false);
      setMessage(`マッピングファイルを読み込みました: ${file}`);
      // 親コンポーネントにロードを通知（ファイルパスを渡す）
      if (onMappingSaved) {
        onMappingSaved(file);
      }
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
      
      // 既存のマッピングから、シーケンスで使っていないボタンの設定を保持
      const currentButtons = mapping.mapping || [];
      const preservedButtons: UserButton[] = currentButtons.filter(btn => !btn.use_in_sequence);
      
      // CSVから新しいボタンを作成
      const newButtons: UserButton[] = buttons.map((btnName, index) => {
        const existing = currentButtons.find(b => b.user_button === btnName);
        return {
          user_button: btnName,
          controller_button: existing?.controller_button || [`button${Math.min(index + 1, 12)}`],
          use_in_sequence: true, // CSVから読み込まれたボタンはシーケンスで使用
        };
      });
      
      // 新しいマッピング作成
      const allButtons = [...newButtons, ...preservedButtons];
      const newMapping: ButtonMapping = {
        controller_type: mapping.controller_type,
        mapping: allButtons,
      };
      
      setMapping(newMapping);
      setCurrentFilePath(""); // ファイルパスをクリア
      setHasUnsavedChanges(false);
      setMessage(`CSVから${buttons.length}個のボタンを検出し、マッピングを作成しました。「名前を付けて保存」で保存してください。`);
    } catch (error) {
      setMessage(`CSV読み込みエラー: ${error}`);
    }
  };

  const handleXboxMappingChange = (xboxButton: string, userButton: string) => {
    setMapping((prev) => {
      const newMapping = prev.mapping.map(btn =>
        btn.user_button === userButton ? { ...btn, controller_button: [xboxButton] } : btn
      );
      return {
        ...prev,
        mapping: newMapping,
      };
    });
    setHasUnsavedChanges(true);
  };

  const removeXboxMapping = (userButton: string) => {
    setMapping((prev) => {
      const newMapping = prev.mapping.filter(btn => btn.user_button !== userButton);
      return {
        ...prev,
        mapping: newMapping,
      };
    });
    setHasUnsavedChanges(true);
  };

  const toggleSequenceButton = (userButton: string) => {
    setMapping((prev) => {
      const newMapping = prev.mapping.map(btn =>
        btn.user_button === userButton ? { ...btn, use_in_sequence: !btn.use_in_sequence } : btn
      );
      return {
        ...prev,
        mapping: newMapping,
      };
    });
    setHasUnsavedChanges(true);
  };

  const addNewMapping = () => {
    const userButton = prompt("ボタン名を入力してください（例: punch, kick, jump）");
    if (userButton && userButton.trim()) {
      const trimmed = userButton.trim();
      const newButton: UserButton = {
        user_button: trimmed,
        controller_button: ["button1"],
        use_in_sequence: false,
      };
      setMapping((prev) => ({
        ...prev,
        mapping: [...prev.mapping, newButton],
      }));
      setHasUnsavedChanges(true);
    }
  };

  const handleTestButtonPress = (_csvButton: string, xboxButton: string) => {
    if (!isConnected) return;
    setActiveTestButton(xboxButton); // Xboxボタン名を設定
  };

  const handleTestButtonRelease = () => {
    setActiveTestButton(null);
  };

  const moveButtonUp = (userButton: string) => {
    setMapping((prev) => {
      const buttons = prev.mapping;
      const index = buttons.findIndex(btn => btn.user_button === userButton);
      if (index <= 0) return prev; // 既に一番上
      
      const newButtons = [...buttons];
      [newButtons[index - 1], newButtons[index]] = [newButtons[index], newButtons[index - 1]];
      
      return {
        ...prev,
        mapping: newButtons,
      };
    });
    setHasUnsavedChanges(true);
  };

  const moveButtonDown = (userButton: string) => {
    setMapping((prev) => {
      const buttons = prev.mapping;
      const index = buttons.findIndex(btn => btn.user_button === userButton);
      if (index < 0 || index >= buttons.length - 1) return prev; // 既に一番下
      
      const newButtons = [...buttons];
      [newButtons[index], newButtons[index + 1]] = [newButtons[index + 1], newButtons[index]];
      
      return {
        ...prev,
        mapping: newButtons,
      };
    });
    setHasUnsavedChanges(true);
  };

  const handleClose = () => {
    if (hasUnsavedChanges) {
      const confirmed = window.confirm("保存されていない変更があります。保存せずに閉じますか？");
      if (!confirmed) return;
    }
    onClose();
  };

  // マウスベースのドラッグ&ドロップ用の状態
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const handleMouseDown = (e: React.MouseEvent, index: number) => {
    // ボタンやセレクトのクリックは無視
    const target = e.target as HTMLElement;
    if (target.tagName === 'BUTTON' || target.tagName === 'SELECT' || target.tagName === 'INPUT') {
      return;
    }
    
    e.preventDefault();
    setDraggedIndex(index);
    setIsDragging(true);
  };

  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!isDragging || draggedIndex === null) return;
    
    // マウス位置からドロップ先のインデックスを計算
    const rows = document.querySelectorAll('.mapping-table-inline tbody tr');
    let newDragOverIndex = draggedIndex;
    
    rows.forEach((row, index) => {
      const rect = row.getBoundingClientRect();
      if (e.clientY >= rect.top && e.clientY <= rect.bottom) {
        newDragOverIndex = index;
      }
    });
    
    setDragOverIndex(newDragOverIndex);
  }, [isDragging, draggedIndex]);

  const handleMouseUp = useCallback(() => {
    if (!isDragging || draggedIndex === null) {
      setIsDragging(false);
      setDraggedIndex(null);
      setDragOverIndex(null);
      return;
    }

    if (dragOverIndex !== null && draggedIndex !== dragOverIndex) {
      setMapping((prev) => {
        const buttons = [...prev.mapping];
        const [draggedItem] = buttons.splice(draggedIndex, 1);
        buttons.splice(dragOverIndex, 0, draggedItem);
        
        return {
          ...prev,
          mapping: buttons,
        };
      });
      setHasUnsavedChanges(true);
    }

    setIsDragging(false);
    setDraggedIndex(null);
    setDragOverIndex(null);
  }, [isDragging, draggedIndex, dragOverIndex]);

  useEffect(() => {
    if (isDragging) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      return () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };
    }
  }, [isDragging, handleMouseMove, handleMouseUp]);

  // マッピング配列を使用
  const userButtons: UserButton[] = mapping.mapping;

  return (
    <div 
      className="button-mapping-editor-overlay" 
      onClick={handleClose}
    >
      <div 
        className="button-mapping-editor-window" 
        onClick={(e) => e.stopPropagation()}
      >
        <div className="editor-header">
          <h2>ボタンマッピング設定{hasUnsavedChanges ? " *" : ""}</h2>
          <button onClick={handleClose} className="close-button">
            ✕
          </button>
        </div>

        <div className="editor-content">
          <div className="editor-controls">
            <button onClick={handleNewMapping} className="btn-file">
              📝 新規
            </button>
            <button onClick={handleMappingFileSelect} className="btn-file">
              📁 既存設定
            </button>
            <button onClick={handleCsvFileSelect} className="btn-file">
              📄 ファイル読込
            </button>
            <button 
              onClick={saveMappingToFile} 
              className="btn-save"
              disabled={!currentFilePath}
              title={!currentFilePath ? "先に「名前を付けて保存」でファイルを作成してください" : "現在のファイルに上書き保存"}
            >
              💾 上書き保存
            </button>
            <button onClick={handleSaveAs} className="btn-save">
              💾 名前を付けて保存
            </button>
            <button onClick={addNewMapping} className="btn-add">
              + ボタン追加
            </button>
          </div>

          {currentFilePath && (
            <div className="current-file-info">
              現在のファイル: {currentFilePath}
            </div>
          )}

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
            <th style={{ width: '30px' }}>🔀</th>
            <th>ボタン名</th>
            <th>→</th>
            <th>コントローラー側ボタン</th>
            <th>シーケンス</th>
            <th>順序</th>
            <th>削除</th>
          </tr>
        </thead>
        <tbody>
          {userButtons.length === 0 ? (
            <tr>
              <td colSpan={7} className="empty-message">
                マッピングがありません。CSVファイルを選択するか、「+ ボタン追加」をクリックしてください。
              </td>
            </tr>
          ) : (
            userButtons.map((userButton, index) => {
              const isActive = activeTestButton === userButton.controller_button[0];
              const isBeingDragged = draggedIndex === index;
              const isDropTarget = dragOverIndex === index && draggedIndex !== index;
              
              return (
                <tr 
                  key={userButton.user_button}
                  onMouseDown={(e) => handleMouseDown(e, index)}
                  className={`${isBeingDragged ? 'dragging-row' : ''} ${isDropTarget ? 'drop-target' : ''}`}
                  style={{
                    backgroundColor: !isBeingDragged && !isDropTarget ? (index % 2 === 0 ? '#f9f9f9' : 'white') : undefined,
                    cursor: isDragging ? 'grabbing' : 'grab',
                    userSelect: 'none',
                  }}
                >
                  <td className="drag-handle" title="ドラッグして並び替え">
                    ⠿
                  </td>
                  <td>
                    <button
                      draggable={false}
                      className={`test-csv-button-inline ${isActive ? 'active' : ''}`}
                      onMouseDown={() => handleTestButtonPress(userButton.user_button, userButton.controller_button[0])}
                      onMouseUp={handleTestButtonRelease}
                      onMouseLeave={handleTestButtonRelease}
                      disabled={!isConnected}
                    >
                      {userButton.user_button}
                    </button>
                  </td>
                  <td>→</td>
                  <td>
                    <select
                      draggable={false}
                      value={userButton.controller_button[0]}
                      onChange={(e) => handleXboxMappingChange(e.target.value, userButton.user_button)}
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
                      draggable={false}
                      type="checkbox"
                      checked={userButton.use_in_sequence}
                      onChange={() => toggleSequenceButton(userButton.user_button)}
                    />
                  </td>
                  <td className="order-buttons">
                    <button
                      draggable={false}
                      onClick={() => moveButtonUp(userButton.user_button)}
                      disabled={index === 0}
                      className="order-button"
                      title="上へ"
                    >
                      ▲
                    </button>
                    <button
                      draggable={false}
                      onClick={() => moveButtonDown(userButton.user_button)}
                      disabled={index === userButtons.length - 1}
                      className="order-button"
                      title="下へ"
                    >
                      ▼
                    </button>
                  </td>
                  <td>
                    <button
                      draggable={false}
                      onClick={() => removeXboxMapping(userButton.user_button)}
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
