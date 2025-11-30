import { useState, useEffect, useRef, useCallback } from "react";
import "./App.css";
import { api } from "./api";
import ButtonMappingEditor from "./ButtonMappingEditor";
import SequenceSelector from "./SequenceSelector";
import SequenceEditor from "./SequenceEditor";
import type { SequenceSlot, InputFrame } from "./types";

function App() {
  // Controller state
  const [isConnected, setIsConnected] = useState(false);

  // FPS state
  const [fps, setFpsState] = useState(60);

  // Playback state
  const [isPlaying, setIsPlaying] = useState(false);
  const [invertHorizontal, setInvertHorizontal] = useState(false);
  const [currentFrame, setCurrentFrame] = useState(0);
  const [totalFrames, setTotalFrames] = useState(0);

  // POV (D-pad) direction - using numpad notation
  const [povDirection, setPovDirection] = useState(5); // 5 = neutral

  // Button states (1-10 for Xbox 360)
  const [activeButton, setActiveButton] = useState<number | null>(null);

  // Button mapping editor state
  const [showMappingEditor, setShowMappingEditor] = useState(false);
  const [useMappingLabels, setUseMappingLabels] = useState(false);
  const [buttonMapping, setButtonMapping] = useState<Record<string, string>>(
    {},
  );
  const [sequenceButtons, setSequenceButtons] = useState<string[]>([]); // シーケンスで使用するボタン
  const [activeTestButton, setActiveTestButton] = useState<string | null>(null); // マッピングエディタの試用ボタン

  // Sequence selector state
  const [showSequenceSelector, setShowSequenceSelector] = useState(false);
  const [sequenceSlots, setSequenceSlots] = useState<(SequenceSlot | null)[]>(
    Array(12).fill(null),
  );
  const [loadingSlot, setLoadingSlot] = useState<number | null>(null);
  const [playingSlot, setPlayingSlot] = useState<number | null>(null);
  const [loopPlayback, setLoopPlayback] = useState(false);

  // Sequence chain state
  const [sequenceChain, setSequenceChain] = useState<number[]>([]); // スロット番号の配列
  const [isPlayingChain, setIsPlayingChain] = useState(false);
  const [currentChainIndex, setCurrentChainIndex] = useState(0);
  const [chainFrameMap, setChainFrameMap] = useState<number[]>([]); // 各シーケンスの開始フレーム位置

  // Sequence editor state (modal)
  const [showSequenceEditor, setShowSequenceEditor] = useState(false);
  const [editingSlotPath, setEditingSlotPath] = useState<string | null>(null);
  const [editingSlotIndex, setEditingSlotIndex] = useState<number | null>(null);
  const [currentPlayingRow, setCurrentPlayingRow] = useState<number>(-1);

  // Refs to hold the latest values for use in interval
  const povDirectionRef = useRef(povDirection);
  const activeButtonRef = useRef(activeButton);
  const activeTestButtonRef = useRef(activeTestButton);

  // Load FPS on mount
  useEffect(() => {
    const loadFps = async () => {
      try {
        const currentFps = await api.getFps();
        setFpsState(currentFps);
      } catch (error) {
        console.error("FPS読み込みエラー:", error);
      }
    };
    loadFps();
  }, []);

  // Update refs whenever state changes
  useEffect(() => {
    povDirectionRef.current = povDirection;
  }, [povDirection]);

  useEffect(() => {
    activeButtonRef.current = activeButton;
  }, [activeButton]);

  useEffect(() => {
    activeTestButtonRef.current = activeTestButton;
  }, [activeTestButton]);

  // Update playback progress and auto-stop when finished
  useEffect(() => {
    const interval = setInterval(async () => {
      if (isPlaying) {
        const [current, total] = await api.getPlaybackProgress();
        setCurrentFrame(current);
        setTotalFrames(total);

        // エディタ表示中は再生中のフレーム番号も取得
        if (showSequenceEditor) {
          try {
            const playingFrame = await api.getCurrentPlayingFrame();
            setCurrentPlayingRow(playingFrame);
          } catch (error) {
            // エラーは無視（再生中でない場合など）
          }
        }

        // 最後まで再生したら自動停止またはループ
        if (current >= total && total > 0) {
          console.log(
            "[Progress Monitor] シーケンス終了検知 - current:",
            current,
            "total:",
            total,
            "isPlayingChain:",
            isPlayingChain,
            "loopPlayback:",
            loopPlayback,
          );

          if (loopPlayback) {
            // ループ再生: Rust側が自動でループするので何もしない
            console.log(
              "[Progress Monitor] ループ再生中 - Rust側が自動でループします（stopPlaybackを呼ばない）",
            );
            // stopPlaybackを呼ばないことで、Rust側のループ処理が実行される
          } else {
            // ループなし: 停止
            await api.stopPlayback();
            if (isPlayingChain) {
              console.log("[Progress Monitor] チェーン再生終了");
              setIsPlayingChain(false);
            }
            setIsPlaying(false);
            setPlayingSlot(null);
          }
        }
      } else {
        // 再生停止時はハイライトをリセットしない（停止位置を保持）
      }
    }, 100);

    return () => clearInterval(interval);
  }, [isPlaying, loopPlayback, isPlayingChain, showSequenceEditor]);

  // Send manual input continuously while connected
  useEffect(() => {
    if (isConnected && !isPlaying) {
      // 10msごとに継続的に送信 (READMEのサンプルと同じ)
      // interval内でrefから最新の状態を参照
      const interval = setInterval(async () => {
        try {
          // Build button states - refから最新の状態を取得
          const currentPovDirection = povDirectionRef.current;
          const currentActiveButton = activeButtonRef.current;
          const currentTestButton = activeTestButtonRef.current;

          const buttons: Record<string, number> = {};

          // メインボタン (1-10) と試用ボタンは排他的
          if (currentActiveButton !== null) {
            buttons[`button${currentActiveButton}`] = 1;
          } else if (currentTestButton !== null) {
            // 試用ボタンは既にXboxボタン名なのでそのまま使用
            buttons[currentTestButton] = 1;
          }

          await api.updateManualInput(currentPovDirection, buttons);
        } catch (error) {
          console.error("❌ Failed to send manual input:", error);
        }
      }, 10);

      return () => clearInterval(interval);
    }
  }, [isConnected, isPlaying]);

  const handleConnect = async () => {
    try {
      await api.connectController("xbox");
      setIsConnected(true);
    } catch (error) {
      console.error("接続エラー:", error);
    }
  };

  const handleDisconnect = async () => {
    try {
      await api.disconnectController();
      setIsConnected(false);
    } catch (error) {
      console.error("切断エラー:", error);
    }
  };

  const handleFpsChange = async (newFps: number) => {
    try {
      await api.setFps(newFps);
      setFpsState(newFps);
    } catch (error) {
      console.error("FPS設定エラー:", error);
    }
  };

  const handleInvertToggle = async (checked: boolean) => {
    setInvertHorizontal(checked);
    // 再生中でなければ、次回の再生用に保存するだけ
    // 再生中に変更しても再生中の動作には影響しない
  };

  const handleLoopToggle = async (checked: boolean) => {
    setLoopPlayback(checked);
    try {
      await api.setLoopPlayback(checked);
    } catch (error) {
      console.error("ループ設定エラー:", error);
    }
  };

  const povDirections = [
    { label: "↖", value: 7 },
    { label: "↑", value: 8 },
    { label: "↗", value: 9 },
    { label: "←", value: 4 },
    null, // 中央はニュートラル（空白）
    { label: "→", value: 6 },
    { label: "↙", value: 1 },
    { label: "↓", value: 2 },
    { label: "↘", value: 3 },
  ];

  const buttons = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

  // マッピングを読み込む関数
  const loadMapping = async () => {
    try {
      const mapping = await api.loadButtonMapping("config/button_mapping.json");
      // Xboxマッピングをbutton1-10からCSVボタン名への逆マップに変換
      const reverseMap: Record<string, string> = {};
      const csvButtons: string[] = [];
      Object.entries(mapping.xbox).forEach(([csvButton, xboxButton]) => {
        reverseMap[xboxButton as string] = csvButton;
        csvButtons.push(csvButton);
      });
      setButtonMapping(reverseMap);

      // シーケンス用ボタンを設定（指定がなければ全ボタン）
      if (mapping.sequenceButtons && mapping.sequenceButtons.length > 0) {
        setSequenceButtons(mapping.sequenceButtons);
      } else {
        setSequenceButtons(csvButtons);
      }
    } catch (error) {
      console.log("マッピング読み込みエラー:", error);
      setButtonMapping({});
      setSequenceButtons([]);
    }
  };

  // シーケンスセレクターを開く
  const openSequenceSelector = (slotIndex: number) => {
    setLoadingSlot(slotIndex);
    setShowSequenceSelector(true);
  };

  // シーケンスを選択
  const handleSequenceSelect = async (
    csvPath: string,
    targetSlot: number,
    isCompatible: boolean,
  ) => {
    try {
      // フレームデータをメモリに読み込む
      const frames = await api.loadFramesForEdit(csvPath);
      const newSlots = [...sequenceSlots];
      newSlots[targetSlot] = {
        path: csvPath,
        frames: frames,
        compatible: isCompatible,
      };
      setSequenceSlots(newSlots);

      console.log(
        `✓ スロット${targetSlot + 1}に${frames.length}フレームを読み込みました (互換性: ${isCompatible ? "✓" : "✗"})`,
      );
    } catch (error) {
      console.error(`読み込みエラー:`, error);
    }
  };

  // シーケンスを再生
  const playSequence = async (slotIndex: number) => {
    const slot = sequenceSlots[slotIndex];
    if (!slot) return;

    // 互換性チェック
    if (!slot.compatible) {
      console.log(`✗ スロット${slotIndex + 1}は互換性がないため再生できません`);
      return;
    }

    try {
      // メモリ上のフレームデータを使用して再生
      await api.loadInputSequence(slot.frames);
      setTotalFrames(slot.frames.length);
      setCurrentFrame(0);

      await api.setInvertHorizontal(invertHorizontal);
      await api.setLoopPlayback(loopPlayback);

      await api.startPlayback();
      setIsPlaying(true);
      setPlayingSlot(slotIndex);
      console.log(
        `▶ スロット${slotIndex + 1}を再生中... (${slot.frames.length}フレーム)`,
      );
    } catch (error) {
      console.error(`再生エラー (スロット${slotIndex + 1}):`, error);
    }
  };

  // シーケンス再生を停止
  const stopSequence = async () => {
    try {
      await api.stopPlayback();
      setIsPlaying(false);
      setCurrentFrame(0);
      setPlayingSlot(null);
      setIsPlayingChain(false);
    } catch (error) {
      console.error(`停止エラー:`, error);
    }
  };

  // シーケンスチェーン: スロットをチェーンに追加（同一スロット複数回OK）
  const addToChain = (slotIndex: number) => {
    if (sequenceChain.length < 20) {
      setSequenceChain([...sequenceChain, slotIndex]);
    }
  };

  // シーケンスチェーン: 選択した要素を削除
  const removeFromChain = (index: number) => {
    setSequenceChain(sequenceChain.filter((_, i) => i !== index));
  };

  // シーケンスチェーン: クリア
  const clearChain = () => {
    setSequenceChain([]);
  };

  // シーケンスチェーン: 並び替え
  const moveChainItem = (fromIndex: number, toIndex: number) => {
    const newChain = [...sequenceChain];
    const [moved] = newChain.splice(fromIndex, 1);
    newChain.splice(toIndex, 0, moved);
    setSequenceChain(newChain);
  };

  // スロットをクリア
  const clearSlot = (slotIndex: number) => {
    const newSlots = [...sequenceSlots];
    newSlots[slotIndex] = null;
    setSequenceSlots(newSlots);

    console.log(`✓ スロット${slotIndex + 1}をクリアしました`);
  };

  // 新規シーケンスを作成
  const createNewSequence = async () => {
    if (sequenceButtons.length === 0) {
      console.error("シーケンス用ボタンが設定されていません");
      return null;
    }

    try {
      // 一時ファイルパスを生成
      const tempPath = `temp_new_sequence_${Date.now()}.csv`;

      // 初期フレームデータ（中立、1フレーム）
      // シーケンス用ボタンのみを含む
      const initialFrames = [
        {
          duration: 1,
          direction: 5,
          buttons: Object.fromEntries(sequenceButtons.map((btn) => [btn, 0])),
        },
      ];

      await api.saveFramesForEdit(tempPath, initialFrames);
      return tempPath;
    } catch (error) {
      console.error("新規シーケンス作成エラー:", error);
      return null;
    }
  };

  // シーケンスチェーン: 全シーケンスを結合して再生
  const playChain = async () => {
    if (sequenceChain.length === 0) return;

    console.log("[playChain] ========== チェーン再生開始 ==========");
    console.log("[playChain] sequenceChain:", JSON.stringify(sequenceChain));
    console.log(
      "[playChain] スロット番号順:",
      sequenceChain.map((idx) => idx + 1),
    );

    // 全シーケンスを結合
    const combinedFrames: InputFrame[] = [];
    const frameMap: number[] = []; // 各シーケンスの開始フレーム位置
    let currentFramePosition = 0;

    for (let i = 0; i < sequenceChain.length; i++) {
      const slotIndex = sequenceChain[i];
      const slot = sequenceSlots[slotIndex];

      if (!slot || !slot.compatible) {
        console.log(`✗ スロット${slotIndex + 1}をスキップ`);
        continue;
      }

      frameMap.push(currentFramePosition);
      console.log(
        `[playChain] シーケンス${i}: スロット${slotIndex + 1}, 開始フレーム: ${currentFramePosition}, フレーム数: ${slot.frames.length}`,
      );

      // フレームを結合
      combinedFrames.push(...slot.frames);

      // 総フレーム数を計算（durationの合計）
      const totalDuration = slot.frames.reduce((sum, f) => sum + f.duration, 0);
      currentFramePosition += totalDuration;
    }

    if (combinedFrames.length === 0) {
      console.log("[playChain] 再生可能なシーケンスがありません");
      return;
    }

    console.log(
      `[playChain] 結合完了: 総フレーム数=${currentFramePosition}, シーケンス数=${frameMap.length}`,
    );
    console.log("[playChain] フレームマップ:", frameMap);

    try {
      // 結合したシーケンスをメモリに読み込む
      await api.loadInputSequence(combinedFrames);
      setTotalFrames(currentFramePosition);
      setCurrentFrame(0);
      setChainFrameMap(frameMap);
      setCurrentChainIndex(0);

      await api.setInvertHorizontal(invertHorizontal);
      await api.setLoopPlayback(loopPlayback);
      await api.startPlayback();
      setIsPlaying(true);
      setIsPlayingChain(true);
      setPlayingSlot(null);

      console.log("[playChain] ========== チェーン再生開始完了 ==========");
    } catch (error) {
      console.error("チェーン再生エラー:", error);
    }
  };

  // チェーン再生中の現在のシーケンスインデックスを更新
  useEffect(() => {
    if (!isPlayingChain || chainFrameMap.length === 0) return;

    // 現在のフレーム位置から、どのシーケンスを再生中か判定
    let newChainIndex = 0;
    for (let i = chainFrameMap.length - 1; i >= 0; i--) {
      if (currentFrame >= chainFrameMap[i]) {
        newChainIndex = i;
        break;
      }
    }

    if (newChainIndex !== currentChainIndex) {
      console.log(
        `[Chain Progress] シーケンス切り替え: ${currentChainIndex} → ${newChainIndex}`,
      );
      setCurrentChainIndex(newChainIndex);
    }
  }, [currentFrame, isPlayingChain, chainFrameMap, currentChainIndex]);

  // チェーン再生終了時の処理
  useEffect(() => {
    if (isPlayingChain && !isPlaying) {
      console.log("[Chain End] チェーン再生終了");
      setIsPlayingChain(false);
      setCurrentChainIndex(0);
      setChainFrameMap([]);
    }
  }, [isPlaying, isPlayingChain]);

  // 初回マウント時にマッピングを読み込む
  useEffect(() => {
    loadMapping();
  }, []);

  return (
    <main className="container">
      <h1>Input Player</h1>

      {/* Manual Input */}
      <section className="section">
        <div className="section-header-with-controls">
          <h2>手動入力</h2>
          <div className="manual-input-controls">
            <div className="fps-control">
              <label htmlFor="fps-select">FPS:</label>
              <select
                id="fps-select"
                value={fps}
                onChange={(e) => handleFpsChange(Number(e.target.value))}
                className="fps-select"
              >
                <option value={30}>30</option>
                <option value={60}>60</option>
                <option value={120}>120</option>
                <option value={144}>144</option>
              </select>
            </div>
            <button
              onClick={() => setShowMappingEditor(!showMappingEditor)}
              className={`btn-small ${showMappingEditor ? "active" : ""}`}
            >
              {showMappingEditor ? "▼" : "▶"} マッピング設定
            </button>
            <button
              onClick={isConnected ? handleDisconnect : handleConnect}
              className={`connection-status-button ${isConnected ? "connected" : "disconnected"}`}
            >
              <span className="status-indicator">
                {isConnected ? "●" : "○"}
              </span>
              <span className="status-text">
                Xbox 360: {isConnected ? "接続中" : "未接続"}
              </span>
            </button>
          </div>
        </div>

        {/* POV / D-pad とボタン */}
        <div className="input-controls-row">
          <div className="pov-section">
            <h3>POV (方向パッド)</h3>
            <div className="pov-grid">
              {povDirections.map((dir, index) =>
                dir === null ? (
                  <div key={`empty-${index}`} className="pov-empty" />
                ) : (
                  <button
                    key={dir.value}
                    className={povDirection === dir.value ? "active" : ""}
                    onMouseDown={() => setPovDirection(dir.value)}
                    onMouseUp={() => setPovDirection(5)}
                    onMouseLeave={() => setPovDirection(5)}
                    disabled={isPlaying}
                  >
                    {dir.label}
                  </button>
                ),
              )}
            </div>
          </div>

          {/* Buttons */}
          <div className="buttons-section">
            <div className="buttons-header">
              <h3>
                ボタン (1-10){" "}
                <span className="button-hint">マウスを押している間だけON</span>
              </h3>
              <label className="mapping-display-checkbox">
                <input
                  type="checkbox"
                  checked={useMappingLabels}
                  onChange={(e) => setUseMappingLabels(e.target.checked)}
                />
                マッピング名で表示
              </label>
            </div>
            <div className="button-grid">
              {buttons.map((btn) => {
                const buttonKey = `button${btn}`;
                const csvButtonName = buttonMapping[buttonKey];
                const isMapped = !!csvButtonName;
                const isSequenceButton =
                  csvButtonName && sequenceButtons.includes(csvButtonName);

                let label = `ボタン ${btn}`;

                // マッピング名表示モードの場合
                if (useMappingLabels && isMapped) {
                  // マッピングされている場合は常にCSV名を表示
                  label = `${csvButtonName} (${btn})`;
                } else {
                  // デフォルトラベル
                  if (btn === 1) label = "A (1)";
                  else if (btn === 2) label = "B (2)";
                  else if (btn === 3) label = "X (3)";
                  else if (btn === 4) label = "Y (4)";
                  else if (btn === 5) label = "LB (5)";
                  else if (btn === 6) label = "RB (6)";
                  else if (btn === 7) label = "LT (7)";
                  else if (btn === 8) label = "RT (8)";
                  else if (btn === 9) label = "BACK (9)";
                  else if (btn === 10) label = "START (10)";
                  else if (btn === 11) label = "LS (11)";
                  else if (btn === 12) label = "RS (12)";
                }

                // クラス名を決定
                // マッピングされている場合のみ色分け
                const buttonClasses = [
                  activeButton === btn ? "active" : "",
                  isMapped
                    ? isSequenceButton
                      ? "sequence-button"
                      : "manual-only-button"
                    : "",
                ]
                  .filter(Boolean)
                  .join(" ");

                return (
                  <button
                    key={btn}
                    className={buttonClasses}
                    onMouseDown={() => setActiveButton(btn)}
                    onMouseUp={() => setActiveButton(null)}
                    onMouseLeave={() => setActiveButton(null)}
                    disabled={isPlaying}
                  >
                    {label}
                  </button>
                );
              })}
            </div>
          </div>
        </div>

        {/* シーケンス再生 */}
        <div className="sequence-section section">
          <div className="sequence-header">
            <h3>シーケンス再生</h3>
            <span className="frame-counter">
              {isPlaying ? `${currentFrame} / ${totalFrames}` : "0 / 0"}{" "}
              フレーム
            </span>
          </div>
          <div className="sequence-controls">
            <div className="sequence-buttons">
              {[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((i) => {
                const slot = sequenceSlots[i];
                const isLoaded = slot !== null;
                const isCompatible = slot?.compatible ?? true;
                const isThisSlotPlaying = isPlaying && playingSlot === i;
                const isOtherSlotPlaying = isPlaying && playingSlot !== i;
                const progress =
                  isThisSlotPlaying && totalFrames > 0
                    ? (currentFrame / totalFrames) * 100
                    : 0;

                return (
                  <button
                    key={i}
                    onClick={() => {
                      if (isPlayingChain) {
                        // チェーン再生中は何もしない
                        return;
                      }
                      if (isThisSlotPlaying) {
                        // 再生中のスロットを再度押すと停止
                        stopSequence();
                      } else if (slot && isCompatible) {
                        // 読み込み済みで互換性があるスロットを押すと再生
                        playSequence(i);
                      } else if (slot && !isCompatible) {
                        // 互換性のないスロットは何もしない
                        console.log(
                          `✗ スロット${i + 1}は互換性がないため再生できません`,
                        );
                      } else {
                        // 空のスロットを押すとファイル選択
                        openSequenceSelector(i);
                      }
                    }}
                    onContextMenu={async (e) => {
                      e.preventDefault();
                      if (e.ctrlKey && isLoaded) {
                        // Ctrl+右クリックで破棄
                        clearSlot(i);
                      } else if (isLoaded && slot) {
                        // スロットが割り当て済みの場合は編集
                        setEditingSlotPath(slot.path);
                        setEditingSlotIndex(i);
                        setShowSequenceEditor(true);
                      } else {
                        // スロット未割り当ての場合は新規作成
                        const tempPath = await createNewSequence();
                        if (tempPath) {
                          setEditingSlotPath(tempPath);
                          setEditingSlotIndex(i);
                          setShowSequenceEditor(true);
                        }
                      }
                    }}
                    className={`btn-sequence ${
                      isThisSlotPlaying
                        ? "playing"
                        : isLoaded && !isCompatible
                          ? "incompatible"
                          : isLoaded
                            ? "loaded"
                            : "empty"
                    }`}
                    disabled={isOtherSlotPlaying || isPlayingChain}
                    title={
                      slot
                        ? `${
                            slot.path
                              .replace(/\\/g, "/")
                              .split("/")
                              .pop()
                              ?.replace(/\.csv$/i, "") || "Unknown"
                          }\n${isCompatible ? "(右クリック: 編集 / Ctrl+右クリック: 破棄)" : "(互換性なし - 再生不可)\n(右クリック: 編集 / Ctrl+右クリック: 破棄)"}`
                        : `スロット${i + 1}\n(左クリック: 選択 / 右クリック: 新規作成)`
                    }
                    style={
                      {
                        "--progress": `${progress}%`,
                      } as React.CSSProperties
                    }
                  >
                    {i + 1}
                  </button>
                );
              })}
            </div>
          </div>
          <div className="sequence-options">
            <label className="invert-control">
              <input
                type="checkbox"
                checked={invertHorizontal}
                onChange={(e) => handleInvertToggle(e.target.checked)}
              />
              左右反転
            </label>
            <label className="loop-control">
              <input
                type="checkbox"
                checked={loopPlayback}
                onChange={(e) => handleLoopToggle(e.target.checked)}
              />
              ループ再生
            </label>
          </div>
        </div>

        {/* シーケンスチェーン */}
        <div className="chain-section section">
          <div className="chain-header">
            <h3>シーケンスチェーン編集</h3>
            {isPlayingChain && (
              <div
                style={{ fontSize: "11px", color: "#888", marginBottom: "4px" }}
              >
                Debug: 再生中シーケンス={currentChainIndex + 1} / 総数=
                {sequenceChain.length} (フレーム: {currentFrame} / {totalFrames}
                )
              </div>
            )}
            <div className="chain-controls">
              <button
                onClick={isPlayingChain ? stopSequence : playChain}
                className="btn-chain-play"
                disabled={
                  sequenceChain.length === 0 || (isPlaying && !isPlayingChain)
                }
              >
                {isPlayingChain ? "■ 停止" : "▶ 再生"}
              </button>
              <button
                onClick={clearChain}
                className="btn-chain-clear"
                disabled={sequenceChain.length === 0 || isPlayingChain}
              >
                🗑 クリア
              </button>
            </div>
          </div>

          {/* チェーン追加用ボタン */}
          <div className="chain-add-buttons">
            <p className="chain-hint">
              クリックでチェーンに追加 (最大20個、同じスロット複数回OK)
            </p>
            <div className="sequence-buttons">
              {[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((i) => {
                const slot = sequenceSlots[i];
                const isLoaded = slot !== null;
                const isCompatible = slot?.compatible ?? true;

                return (
                  <button
                    key={`chain-add-${i}`}
                    onClick={() => {
                      if (isLoaded && sequenceChain.length < 20) {
                        addToChain(i);
                      }
                    }}
                    className={`btn-sequence ${
                      isLoaded && !isCompatible
                        ? "incompatible"
                        : isLoaded
                          ? "loaded"
                          : "empty"
                    }`}
                    disabled={
                      !isLoaded || sequenceChain.length >= 20 || isPlayingChain
                    }
                    title={
                      slot
                        ? `${
                            slot.path
                              .replace(/\\/g, "/")
                              .split("/")
                              .pop()
                              ?.replace(/\.csv$/i, "") || "Unknown"
                          }\n(クリックでチェーンに追加)`
                        : `スロット${i + 1}\n(空)`
                    }
                  >
                    {i + 1}
                  </button>
                );
              })}
            </div>
          </div>

          {/* チェーン表示エリア */}
          <div className="chain-display-area">
            <h4>現在のチェーン ({sequenceChain.length}/20)</h4>
            {sequenceChain.length === 0 ? (
              <div className="chain-empty-message">
                上のボタンをクリックしてチェーンを作成
              </div>
            ) : (
              <div className="chain-buttons">
                {sequenceChain.map((slotIndex, chainIndex) => {
                  const slot = sequenceSlots[slotIndex];
                  const isCompatible = slot?.compatible ?? true;
                  const isCurrentlyPlaying =
                    isPlayingChain && currentChainIndex === chainIndex;

                  return (
                    <button
                      key={`chain-${chainIndex}`}
                      draggable={!isPlaying && !isPlayingChain}
                      onDragStart={(e) => {
                        e.dataTransfer.setData(
                          "text/plain",
                          chainIndex.toString(),
                        );
                        e.dataTransfer.effectAllowed = "move";
                      }}
                      onDragOver={(e) => {
                        e.preventDefault();
                        e.dataTransfer.dropEffect = "move";
                      }}
                      onDrop={(e) => {
                        e.preventDefault();
                        const fromIndex = parseInt(
                          e.dataTransfer.getData("text/plain"),
                        );
                        if (!isNaN(fromIndex) && fromIndex !== chainIndex) {
                          moveChainItem(fromIndex, chainIndex);
                        }
                      }}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        if (!isPlayingChain) {
                          removeFromChain(chainIndex);
                        }
                      }}
                      className={`btn-sequence ${
                        isCurrentlyPlaying
                          ? "playing"
                          : !isCompatible
                            ? "incompatible"
                            : "loaded"
                      }`}
                      title={`${
                        slot?.path
                          .replace(/\\/g, "/")
                          .split("/")
                          .pop()
                          ?.replace(/\.csv$/i, "") || "Unknown"
                      }\nスロット: ${slotIndex + 1}\n順序: ${chainIndex + 1}\n(ドラッグで並び替え / 右クリックで削除)`}
                    >
                      {slotIndex + 1}
                      <span
                        style={{
                          fontSize: "9px",
                          display: "block",
                          color: "#888",
                        }}
                      >
                        [{chainIndex}]
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </section>

      {/* Button Mapping Editor - 展開式 */}
      {showMappingEditor && (
        <ButtonMappingEditor
          onClose={() => {
            setShowMappingEditor(false);
            loadMapping();
          }}
          initialConnected={isConnected}
          isExpanded={true}
          activeTestButton={activeTestButton}
          setActiveTestButton={setActiveTestButton}
          onMappingSaved={loadMapping}
        />
      )}

      {/* Sequence Selector Modal */}
      {showSequenceSelector && (
        <SequenceSelector
          onClose={() => setShowSequenceSelector(false)}
          onSelect={(path, slot, compatible) =>
            handleSequenceSelect(path, slot, compatible)
          }
          availableButtons={sequenceButtons}
          targetSlot={loadingSlot}
          currentSlots={sequenceSlots}
        />
      )}

      {/* Sequence Editor Modal */}
      {showSequenceEditor && editingSlotPath && (
        <SequenceEditor
          csvPath={editingSlotPath}
          onClose={async (savedPath) => {
            // 保存されたパスがある場合、編集していたスロットに割り当て
            if (savedPath && editingSlotIndex !== null) {
              try {
                // フレームデータをメモリに読み込む
                const frames = await api.loadFramesForEdit(savedPath);
                const newSlots = [...sequenceSlots];
                newSlots[editingSlotIndex] = {
                  path: savedPath,
                  frames: frames,
                  compatible: true, // 編集後は互換性があると仮定
                };
                setSequenceSlots(newSlots);
                console.log(
                  `✓ スロット${editingSlotIndex + 1}を更新しました (${frames.length}フレーム)`,
                );
              } catch (error) {
                console.error("シーケンス更新エラー:", error);
              }
            }
            setShowSequenceEditor(false);
            setEditingSlotPath(null);
            setEditingSlotIndex(null);
            setCurrentPlayingRow(-1);
          }}
          currentPlayingRow={currentPlayingRow}
          sequenceButtons={sequenceButtons}
        />
      )}
    </main>
  );
}

export default App;
