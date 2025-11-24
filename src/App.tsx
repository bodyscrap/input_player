import { useState, useEffect, useRef } from "react";
import "./App.css";
import { api } from "./api";
import ButtonMappingEditor from "./ButtonMappingEditor";
import SequenceSelector from "./SequenceSelector";

function App() {
  // Controller state
  const [isConnected, setIsConnected] = useState(false);

  // FPS state
  const [fps, setFpsState] = useState(60);

  // Playback state
  const [isPlaying, setIsPlaying] = useState(false);
  const [invertHorizontal, setInvertHorizontal] = useState(false);
  const [startPlaybackInverted, setStartPlaybackInverted] = useState(false); // 再生開始時の左右反転状態
  const [currentFrame, setCurrentFrame] = useState(0);
  const [totalFrames, setTotalFrames] = useState(0);

  // POV (D-pad) direction - using numpad notation
  const [povDirection, setPovDirection] = useState(5); // 5 = neutral

  // Button states (1-10 for Xbox 360)
  const [activeButton, setActiveButton] = useState<number | null>(null);
  
  // Button mapping editor state
  const [showMappingEditor, setShowMappingEditor] = useState(false);
  const [useMappingLabels, setUseMappingLabels] = useState(false);
  const [buttonMapping, setButtonMapping] = useState<Record<string, string>>({});
  const [availableButtons, setAvailableButtons] = useState<string[]>([]);
  const [activeTestButton, setActiveTestButton] = useState<string | null>(null); // マッピングエディタの試用ボタン
  
  // Sequence selector state
  const [showSequenceSelector, setShowSequenceSelector] = useState(false);
  const [sequenceSlots, setSequenceSlots] = useState<(string | null)[]>([null, null, null, null]);
  const [loadingSlot, setLoadingSlot] = useState<number | null>(null);
  const [playingSlot, setPlayingSlot] = useState<number | null>(null);
  
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
        
        // 最後まで再生したら自動停止
        if (current >= total && total > 0) {
          await api.stopPlayback();
          setIsPlaying(false);
          setPlayingSlot(null);
        }
      }
    }, 100);

    return () => clearInterval(interval);
  }, [isPlaying]);

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
      setAvailableButtons(csvButtons);
    } catch (error) {
      console.log("マッピング読み込みエラー:", error);
      setButtonMapping({});
      setAvailableButtons([]);
    }
  };

  // シーケンスセレクターを開く
  const openSequenceSelector = (slotIndex: number) => {
    setLoadingSlot(slotIndex);
    setShowSequenceSelector(true);
  };

  // シーケンスを選択
  const handleSequenceSelect = async (csvPath: string, targetSlot: number) => {
    try {
      const frameCount = await api.loadInputFile(csvPath);
      const newSlots = [...sequenceSlots];
      newSlots[targetSlot] = csvPath;
      setSequenceSlots(newSlots);
      setTotalFrames(frameCount);
      alert(`スロット${targetSlot + 1}に${frameCount}フレームを読み込みました`);
    } catch (error) {
      alert(`読み込みエラー: ${error}`);
    }
  };

  // シーケンスを再生
  const playSequence = async (slotIndex: number) => {
    const csvPath = sequenceSlots[slotIndex];
    if (!csvPath) return;
    
    try {
      const frameCount = await api.loadInputFile(csvPath);
      setTotalFrames(frameCount);
      setCurrentFrame(0);
      
      // 再生開始時の左右反転状態を保存
      setStartPlaybackInverted(invertHorizontal);
      await api.setInvertHorizontal(invertHorizontal);
      
      await api.startPlayback();
      setIsPlaying(true);
      setPlayingSlot(slotIndex);
    } catch (error) {
      alert(`再生エラー: ${error}`);
    }
  };

  // シーケンス再生を停止
  const stopSequence = async () => {
    try {
      await api.stopPlayback();
      setIsPlaying(false);
      setCurrentFrame(0);
      setPlayingSlot(null);
    } catch (error) {
      alert(`停止エラー: ${error}`);
    }
  };

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
              className={`btn-small ${showMappingEditor ? 'active' : ''}`}
            >
              {showMappingEditor ? '▼' : '▶'} マッピング設定
            </button>
            <button 
              onClick={isConnected ? handleDisconnect : handleConnect}
              className={`connection-status-button ${isConnected ? 'connected' : 'disconnected'}`}
            >
              <span className="status-indicator">{isConnected ? '●' : '○'}</span>
              <span className="status-text">Xbox 360: {isConnected ? '接続中' : '未接続'}</span>
            </button>
          </div>
        </div>

        {/* POV / D-pad とシーケンス再生 */}
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
                  )
                )}
              </div>
            </div>

          {/* シーケンス再生 */}
          <div className="sequence-section">
            <div className="sequence-header">
              <h3>シーケンス再生</h3>
              <span className="frame-counter">
                {isPlaying ? `${currentFrame} / ${totalFrames}` : '0 / 0'} フレーム
              </span>
            </div>
            <div className="sequence-controls">
              <div className="sequence-buttons">
                <button 
                  onClick={() => openSequenceSelector(0)} 
                  className="btn-sequence"
                  title="入力履歴読込"
                  disabled={isPlaying}
                >
                  📂
                </button>
                {[0, 1, 2, 3].map((i) => {
                  const isLoaded = sequenceSlots[i] !== null;
                  const isThisSlotPlaying = isPlaying && playingSlot === i;
                  const isOtherSlotPlaying = isPlaying && playingSlot !== i;
                  const progress = isThisSlotPlaying && totalFrames > 0 ? (currentFrame / totalFrames) * 100 : 0;
                  
                  return (
                    <button
                      key={i}
                      onClick={() => {
                        if (isThisSlotPlaying) {
                          // 再生中のスロットを再度押すと停止
                          stopSequence();
                        } else if (sequenceSlots[i]) {
                          // 読み込み済みのスロットを押すと再生
                          playSequence(i);
                        } else {
                          // 空のスロットを押すとファイル選択
                          openSequenceSelector(i);
                        }
                      }}
                      className={`btn-sequence ${
                        isThisSlotPlaying ? 'playing' : 
                        isLoaded ? 'loaded' : 
                        'empty'
                      }`}
                      disabled={isOtherSlotPlaying}
                      title={sequenceSlots[i] || `スロット${i + 1}`}
                      style={{
                        '--progress': `${progress}%`
                      } as React.CSSProperties}
                    >
                      {i + 1}
                    </button>
                  );
                })}
              </div>
            </div>
            <label className="invert-control">
              <input
                type="checkbox"
                checked={invertHorizontal}
                onChange={(e) => handleInvertToggle(e.target.checked)}
              />
              左右反転
            </label>
          </div>
        </div>

            {/* Buttons */}
            <div className="buttons-section">
              <div className="buttons-header">
                <h3>ボタン (1-10)</h3>
                <label className="mapping-display-checkbox">
                  <input
                    type="checkbox"
                    checked={useMappingLabels}
                    onChange={(e) => setUseMappingLabels(e.target.checked)}
                  />
                  マッピング名で表示
                </label>
              </div>
              <p className="button-hint">マウスを押している間だけON</p>
              <div className="button-grid">
                {buttons.map((btn) => {
                  const buttonKey = `button${btn}`;
                  let label = `ボタン ${btn}`;
                  
                  // マッピングを使用する場合、CSVボタン名を表示
                  if (useMappingLabels && buttonMapping[buttonKey]) {
                    label = `${buttonMapping[buttonKey]} (${btn})`;
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
                  
                  return (
                    <button
                      key={btn}
                      className={activeButton === btn ? "active" : ""}
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
          />
        )}
      
      {/* Sequence Selector Modal */}
      {showSequenceSelector && (
        <SequenceSelector
          onClose={() => setShowSequenceSelector(false)}
          onSelect={handleSequenceSelect}
          availableButtons={availableButtons}
          targetSlot={loadingSlot}
          currentSlots={sequenceSlots}
        />
      )}
    </main>
  );
}

export default App;
