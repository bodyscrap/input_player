import { useState, useEffect, useRef } from "react";
import "./SequenceEditor.css";

interface InputFrame {
  duration: number;
  direction: number;
  buttons: Record<string, number>;
}

interface SequenceEditorProps {
  csvPath: string;
  onClose: () => void;
  currentPlayingRow: number | null; // 現在再生中の行（外部から制御）
  sequenceButtons: string[]; // シーケンスで使用可能なボタンのリスト
}

function SequenceEditor({ csvPath, onClose, currentPlayingRow, sequenceButtons }: SequenceEditorProps) {
  console.log("========== SequenceEditor component created ==========");
  console.log("Props - csvPath:", csvPath);
  console.log("Props - currentPlayingRow:", currentPlayingRow);
  
  const [frames, setFrames] = useState<InputFrame[]>([]);
  const [selectedRows, setSelectedRows] = useState<Set<number>>(new Set());
  const [selectedRow, setSelectedRow] = useState<number | null>(null);
  const [buttonNames, setButtonNames] = useState<string[]>([]);
  const [message, setMessage] = useState("");
  const [hasChanges, setHasChanges] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [localIsPlaying, setLocalIsPlaying] = useState(false);
  const [internalPlayingRow, setInternalPlayingRow] = useState<number>(-1);
  
  // 再生中かどうかを判定（ローカル状態または外部状態）
  const isPlaying = localIsPlaying || (currentPlayingRow !== null && currentPlayingRow >= 0);
  // 表示用の行番号: internalPlayingRowが有効(-1以外)ならそれを使用、そうでなければcurrentPlayingRow
  const displayPlayingRow = internalPlayingRow >= 0 ? internalPlayingRow : currentPlayingRow;

  console.log("[SequenceEditor] State:", {
    localIsPlaying,
    internalPlayingRow,
    currentPlayingRow,
    isPlaying,
    displayPlayingRow
  });

  useEffect(() => {
    console.log("Loading frames for:", csvPath);
    loadFrames();
  }, [csvPath]);

  // localIsPlayingの変更を監視
  useEffect(() => {
    console.log("[SequenceEditor] localIsPlaying変更:", localIsPlaying);
  }, [localIsPlaying]);

  // 現在再生中の行にスクロール
  useEffect(() => {
    const playingRow = displayPlayingRow;
    if (playingRow !== null && playingRow >= 0 && scrollRef.current) {
      const rowElement = scrollRef.current.querySelector(`[data-row="${playingRow}"]`);
      if (rowElement) {
        rowElement.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }
  }, [displayPlayingRow]);

  // 再生中は再生状態をポーリング
  useEffect(() => {
    console.log("[SequenceEditor] ポーリング useEffect - localIsPlaying:", localIsPlaying);
    if (!localIsPlaying) return;
    
    console.log("[SequenceEditor] ポーリング開始");
    const interval = setInterval(async () => {
      try {
        const { api } = await import("./api");
        const playing = await api.isPlaying();
        if (!playing) {
          // 再生が終了した
          // 最終フレームのハイライトを保持
          const finalFrame = frames.length - 1;
          console.log("[SequenceEditor] 再生終了検知 - 最終フレームに設定:", finalFrame);
          setInternalPlayingRow(finalFrame);
          setLocalIsPlaying(false);
          setMessage("再生が終了しました");
        } else {
          // 現在のフレームを取得
          try {
            const frame = await api.getCurrentPlayingFrame();
            if (frame !== internalPlayingRow) {
              console.log("[SequenceEditor] フレーム更新:", internalPlayingRow, "→", frame);
            }
            setInternalPlayingRow(frame);
          } catch (e) {
            console.error("[SequenceEditor] フレーム取得エラー:", e);
          }
        }
      } catch (error) {
        console.error("[SequenceEditor] 再生状態の確認エラー:", error);
      }
    }, 16); // 約60FPS (16ms)ごとにチェック
    
    return () => {
      console.log("[SequenceEditor] ポーリング停止");
      clearInterval(interval);
    };
  }, [localIsPlaying]);

  const loadFrames = async () => {
    console.log("loadFrames called for:", csvPath);
    try {
      const { api } = await import("./api");
      console.log("API imported, calling loadFramesForEdit...");
      const loadedFrames = await api.loadFramesForEdit(csvPath);
      console.log("✓ Frames loaded:", loadedFrames.length, "frames");
      
      // シーケンスボタンのみを使用（非破壊的にソート）
      const buttonNamesArray = [...sequenceButtons].sort();
      console.log("✓ Button names (sequence buttons only):", buttonNamesArray);
      
      // 各フレームのボタンをsequenceButtonsのみに制限
      const filteredFrames = loadedFrames.map((frame: InputFrame) => ({
        ...frame,
        buttons: Object.fromEntries(
          buttonNamesArray.map(btn => [btn, frame.buttons[btn] ?? 0])
        )
      }));
      
      setFrames(filteredFrames);
      setButtonNames(buttonNamesArray);
      setMessage(`${csvPath}を読み込みました (${loadedFrames.length}行)`);
      console.log("✓ State updated successfully");
    } catch (error) {
      console.error("✗ loadFrames error:", error);
      setMessage(`エラー: ${error}`);
    }
  };

  const addRow = (afterIndex: number | null) => {
    if (isPlaying) {
      setMessage("再生中は編集できません");
      return;
    }
    const newFrame: InputFrame = {
      duration: 1,
      direction: 5,
      buttons: Object.fromEntries(sequenceButtons.map(btn => [btn, 0])),
    };
    
    console.log("[addRow] 新規フレーム作成:", newFrame);
    console.log("[addRow] sequenceButtons:", sequenceButtons);
    
    const newFrames = [...frames];
    const insertIndex = afterIndex !== null ? afterIndex + 1 : frames.length;
    newFrames.splice(insertIndex, 0, newFrame);
    
    console.log("[addRow] 更新後のフレーム数:", newFrames.length);
    
    setFrames(newFrames);
    setHasChanges(true);
    setMessage("行を追加しました");
  };

  const deleteSelected = () => {
    if (isPlaying) {
      setMessage("再生中は編集できません");
      return;
    }
    if (selectedRows.size === 0) {
      setMessage("削除する行を選択してください");
      return;
    }
    
    if (frames.length - selectedRows.size < 1) {
      setMessage("最低1行は残す必要があります");
      return;
    }
    
    const indices = Array.from(selectedRows).sort((a, b) => b - a);
    const newFrames = frames.filter((_, i) => !selectedRows.has(i));
    
    // ハイライト位置が削除された行より後ろにある場合は調整
    if (internalPlayingRow >= 0) {
      const deletedBeforePlayingRow = Array.from(selectedRows).filter(idx => idx < internalPlayingRow).length;
      const newPlayingRow = internalPlayingRow - deletedBeforePlayingRow;
      
      // 削除された行がハイライト行自身だった場合はハイライトを削除
      if (selectedRows.has(internalPlayingRow)) {
        setInternalPlayingRow(-1);
      } else if (newPlayingRow >= newFrames.length) {
        // 新しい位置が範囲外の場合は最後の行
        setInternalPlayingRow(newFrames.length - 1);
      } else {
        setInternalPlayingRow(newPlayingRow);
      }
    }
    
    setFrames(newFrames);
    setSelectedRows(new Set());
    setSelectedRow(null);
    setHasChanges(true);
    setMessage(`${indices.length}行を削除しました`);
  };

  const handleSave = async () => {
    if (isPlaying) {
      setMessage("再生中は保存できません");
      return;
    }
    
    // 一時ファイル（新規作成）の場合は別名保存を実行
    if (csvPath.startsWith("temp_new_sequence_")) {
      await handleSaveAs();
      return;
    }
    
    try {
      const { api } = await import("./api");
      // 元のファイルに直接上書き
      await api.saveFramesForEdit(csvPath, frames);
      setHasChanges(false);
      setMessage("✓ 保存しました");
    } catch (error) {
      setMessage(`保存エラー: ${error}`);
    }
  };

  const handleSaveAs = async () => {
    if (isPlaying) {
      setMessage("再生中は保存できません");
      return;
    }
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      
      // ファイル名の初期値を元のパスから取得
      const fileName = csvPath.split(/[\\/]/).pop() || "sequence.csv";
      
      // 保存ダイアログを表示
      const savePath = await save({
        defaultPath: fileName,
        filters: [{
          name: "CSV Files",
          extensions: ["csv"]
        }]
      });
      
      if (!savePath) {
        setMessage("保存がキャンセルされました");
        return;
      }
      
      const { api } = await import("./api");
      await api.saveFramesForEdit(savePath, frames);
      setMessage(`✓ 別名保存しました: ${savePath}`);
    } catch (error) {
      setMessage(`保存エラー: ${error}`);
    }
  };

  const handlePlayStop = async () => {
    if (isPlaying) {
      // 停止処理
      try {
        const { api } = await import("./api");
        console.log("[SequenceEditor] 停止前 - internalPlayingRow:", internalPlayingRow);
        await api.stopPlayback();
        setLocalIsPlaying(false);
        console.log("[SequenceEditor] 停止後 - internalPlayingRowを保持:", internalPlayingRow);
        setMessage("再生を停止しました");
      } catch (error) {
        console.error("停止エラー:", error);
        setMessage(`停止エラー: ${error}`);
      }
    } else {
      // 再生処理
      try {
        const { api } = await import("./api");
        console.log("[SequenceEditor] 再生開始 - フレーム数:", frames.length);
        console.log("[SequenceEditor] 最初のフレーム:", frames[0]);
        console.log("[SequenceEditor] 最後のフレーム:", frames[frames.length - 1]);
        
        // 編集内容を一時的に保存してから再生
        console.log("[SequenceEditor] 保存中...");
        await api.saveFramesForEdit(csvPath, frames);
        console.log("[SequenceEditor] 保存完了");
        
        console.log("[SequenceEditor] 読み込み中...");
        const frameCount = await api.loadInputFile(csvPath);
        console.log("[SequenceEditor] 読み込み完了 - フレーム数:", frameCount);
        
        if (frameCount !== frames.length) {
          console.warn("[SequenceEditor] ⚠️ フレーム数不一致! 保存:", frames.length, "読み込み:", frameCount);
        }
        
        console.log("[SequenceEditor] 再生開始API呼び出し前");
        await api.startPlayback();
        console.log("[SequenceEditor] 再生開始API呼び出し後");
        
        console.log("[SequenceEditor] setLocalIsPlaying(true) 呼び出し");
        setLocalIsPlaying(true);
        console.log("[SequenceEditor] setInternalPlayingRow(0) 呼び出し");
        setInternalPlayingRow(0);
        console.log("[SequenceEditor] 状態更新完了");
        setMessage(`再生を開始しました (${frameCount}フレーム)`);
      } catch (error) {
        console.error("[SequenceEditor] ❌ 再生エラー:", error);
        console.error("[SequenceEditor] エラー詳細:", JSON.stringify(error, null, 2));
        setMessage(`再生エラー: ${error}`);
        alert(`再生エラーが発生しました: ${error}`);
      }
    }
  };

  const handleReload = async () => {
    try {
      const { api } = await import("./api");
      await api.reloadCurrentSequence();
      setMessage("✓ スロットに反映しました");
    } catch (error) {
      console.error("反映エラー:", error);
      setMessage(`反映エラー: ${error}`);
    }
  };

  const handleRowClick = (index: number, event: React.MouseEvent) => {
    if (event.ctrlKey) {
      const newSelected = new Set(selectedRows);
      if (newSelected.has(index)) {
        newSelected.delete(index);
      } else {
        newSelected.add(index);
      }
      setSelectedRows(newSelected);
    } else if (event.shiftKey && selectedRow !== null) {
      const start = Math.min(selectedRow, index);
      const end = Math.max(selectedRow, index);
      const newSelected = new Set<number>();
      for (let i = start; i <= end; i++) {
        newSelected.add(i);
      }
      setSelectedRows(newSelected);
    } else {
      setSelectedRows(new Set([index]));
      setSelectedRow(index);
    }
  };

  const handleAddRow = () => {
    addRow(frames.length - 1); // 最終行の後に追加
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    // ESCキーで閉じる
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }
    
    // スペースキーで最終行に追加
    if (event.key === ' ' && !event.ctrlKey && !event.shiftKey && !event.altKey) {
      // input要素にフォーカスがある場合は無視
      if (event.target instanceof HTMLInputElement) {
        return;
      }
      event.preventDefault();
      handleAddRow();
      return;
    }
    
    if (event.key === 'Delete') {
      deleteSelected();
    }
  };

  const directionArrows: Record<number, string> = {
    1: "↙", 2: "↓", 3: "↘",
    4: "←", 5: "N", 6: "→",
    7: "↖", 8: "↑", 9: "↗",
  };

  return (
    <div className="sequence-editor-overlay" onClick={onClose}>
      <div className="sequence-editor-window" onClick={(e) => e.stopPropagation()} onKeyDown={handleKeyDown} tabIndex={0}>
        <div className="editor-header">
          <h2>シーケンス編集</h2>
          <div style={{fontSize: '11px', color: '#888', marginBottom: '4px'}}>
            Debug: playing={localIsPlaying ? 'true' : 'false'}, row={internalPlayingRow}, frames={frames.length}
          </div>
          <div className="editor-header-buttons">
            <button onClick={handleSave} disabled={!hasChanges || isPlaying} className="btn-save">
              💾 保存
            </button>
            <button onClick={handleSaveAs} disabled={isPlaying} className="btn-save">
              💾️ 別名保存
            </button>
            <button 
              onClick={handleReload} 
              disabled={hasChanges || isPlaying} 
              className="btn-reload" 
              title={hasChanges ? "保存後に反映できます" : "保存済みの内容をスロットに反映"}
            >
              🔄 スロットに反映
            </button>
            <button onClick={onClose} className="btn-close">✕</button>
          </div>
        </div>

        <div className="editor-toolbar">
          <div className="toolbar-left">
            <button onClick={handlePlayStop} className={`btn-toolbar ${isPlaying ? 'btn-stop' : 'btn-play'}`}>
              {isPlaying ? '■ 停止' : '▶ 再生'}
            </button>
            <div className="toolbar-divider"></div>
            <button onClick={() => addRow(selectedRow)} disabled={isPlaying} className="btn-toolbar">
              ➕ 行追加
            </button>
            <button onClick={deleteSelected} disabled={selectedRows.size === 0 || isPlaying} className="btn-toolbar">
              ❌ 削除 (Del)
            </button>
          </div>
          <div className="toolbar-right">
            <span className="editor-message">{message}</span>
            <span className="editor-status">
              総行数: {frames.length} {hasChanges && "(未保存)"} {isPlaying && "🔴 再生中"}
            </span>
          </div>
        </div>

        <div className="editor-content" ref={scrollRef}>
          <div className="editor-table-wrapper">
            <table className="editor-table">
              <thead>
                <tr>
                  <th className="col-select">選択</th>
                  <th className="col-duration">持続F</th>
                  <th className="col-direction">方向</th>
                  {buttonNames.map((name) => (
                    <th key={name} className="col-button">{name}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {frames.map((frame, index) => {
                  const isSelected = selectedRows.has(index);
                  const isPlayingThisRow = displayPlayingRow === index;
                  
                  return (
                    <tr
                      key={index}
                      data-row={index}
                      className={`${isSelected ? 'selected' : ''} ${isPlayingThisRow ? 'playing' : ''}`}
                      onClick={(e) => handleRowClick(index, e)}
                    >
                      <td className="col-select">{index + 1}</td>
                      <td className="col-duration">
                        <input
                          type="number"
                          value={frame.duration}
                          min={1}
                          disabled={isPlaying}
                          onChange={(e) => {
                            const newFrames = [...frames];
                            newFrames[index].duration = Math.max(1, parseInt(e.target.value) || 1);
                            setFrames(newFrames);
                            setHasChanges(true);
                          }}
                          onClick={(e) => e.stopPropagation()}
                        />
                      </td>
                      <td className="col-direction">
                        <select
                          value={frame.direction}
                          disabled={isPlaying}
                          onChange={(e) => {
                            const newFrames = [...frames];
                            newFrames[index].direction = parseInt(e.target.value);
                            setFrames(newFrames);
                            setHasChanges(true);
                          }}
                          onClick={(e) => e.stopPropagation()}
                        >
                          {[1, 2, 3, 4, 5, 6, 7, 8, 9].map((dir) => (
                            <option key={dir} value={dir}>
                              {directionArrows[dir]}
                            </option>
                          ))}
                        </select>
                      </td>
                      {buttonNames.map((name) => (
                        <td key={name} className="col-button">
                          <input
                            type="checkbox"
                            checked={frame.buttons[name] === 1}
                            disabled={isPlaying}
                            onChange={(e) => {
                              const newFrames = [...frames];
                              newFrames[index].buttons[name] = e.target.checked ? 1 : 0;
                              setFrames(newFrames);
                              setHasChanges(true);
                            }}
                            onClick={(e) => e.stopPropagation()}
                          />
                        </td>
                      ))}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
}

export default SequenceEditor;
