import { useState, useEffect } from "react";
import "./App.css";
import { api } from "./api";
import type { ControllerType, ButtonMapping } from "./types";

function App() {
  // Controller state
  const [controllerType, setControllerType] = useState<ControllerType>("xbox");
  const [isConnected, setIsConnected] = useState(false);
  const [connectionMessage, setConnectionMessage] = useState("");

  // Playback state
  const [isPlaying, setIsPlaying] = useState(false);
  const [invertHorizontal, setInvertHorizontal] = useState(false);
  const [currentFrame, setCurrentFrame] = useState(0);
  const [totalFrames, setTotalFrames] = useState(0);

  // Manual input state
  const [direction, setDirection] = useState(5);
  const [buttonStates, setButtonStates] = useState<Record<string, number>>({
    punch: 0,
    kick: 0,
    jump: 0,
    special: 0,
  });

  // Button mapping state
  const [buttonMapping, setButtonMapping] = useState<ButtonMapping | null>(null);

  // Load button mapping on mount
  useEffect(() => {
    loadMapping();
  }, []);

  // Update playback progress
  useEffect(() => {
    const interval = setInterval(async () => {
      if (isPlaying) {
        const [current, total] = await api.getPlaybackProgress();
        setCurrentFrame(current);
        setTotalFrames(total);
      }
    }, 100);

    return () => clearInterval(interval);
  }, [isPlaying]);

  const loadMapping = async () => {
    try {
      const mapping = await api.loadButtonMapping("button_mapping.json");
      setButtonMapping(mapping);
    } catch (error) {
      console.error("Failed to load button mapping:", error);
    }
  };

  const handleConnect = async () => {
    try {
      const message = await api.connectController(controllerType);
      setIsConnected(true);
      setConnectionMessage(message);
    } catch (error) {
      setConnectionMessage(`接続エラー: ${error}`);
    }
  };

  const handleDisconnect = async () => {
    try {
      const message = await api.disconnectController();
      setIsConnected(false);
      setConnectionMessage(message);
    } catch (error) {
      setConnectionMessage(`切断エラー: ${error}`);
    }
  };

  const handleLoadFile = async () => {
    try {
      // For now, load the sample file
      const frameCount = await api.loadInputFile(
        "sample_input/input_sample_01_input_history.csv"
      );
      setTotalFrames(frameCount);
      alert(`${frameCount}フレームを読み込みました`);
    } catch (error) {
      alert(`ファイル読み込みエラー: ${error}`);
    }
  };

  const handleStartPlayback = async () => {
    try {
      await api.startPlayback();
      setIsPlaying(true);
    } catch (error) {
      alert(`再生開始エラー: ${error}`);
    }
  };

  const handleStopPlayback = async () => {
    try {
      await api.stopPlayback();
      setIsPlaying(false);
      setCurrentFrame(0);
    } catch (error) {
      alert(`再生停止エラー: ${error}`);
    }
  };

  const handlePausePlayback = async () => {
    try {
      await api.pausePlayback();
      setIsPlaying(false);
    } catch (error) {
      alert(`一時停止エラー: ${error}`);
    }
  };

  const handleResumePlayback = async () => {
    try {
      await api.resumePlayback();
      setIsPlaying(true);
    } catch (error) {
      alert(`再開エラー: ${error}`);
    }
  };

  const handleInvertToggle = async (checked: boolean) => {
    setInvertHorizontal(checked);
    await api.setInvertHorizontal(checked);
  };

  const handleDirectionChange = (newDirection: number) => {
    setDirection(newDirection);
    api.updateManualInput(newDirection, buttonStates);
  };

  const handleButtonToggle = (buttonName: string) => {
    const newState = { ...buttonStates };
    newState[buttonName] = newState[buttonName] === 0 ? 1 : 0;
    setButtonStates(newState);
    api.updateManualInput(direction, newState);
  };

  const directionPad = [
    { label: "↖", value: 7 },
    { label: "↑", value: 8 },
    { label: "↗", value: 9 },
    { label: "←", value: 4 },
    { label: "◯", value: 5 },
    { label: "→", value: 6 },
    { label: "↙", value: 1 },
    { label: "↓", value: 2 },
    { label: "↘", value: 3 },
  ];

  return (
    <main className="container">
      <h1>Input Player</h1>

      {/* Controller Connection */}
      <section className="section">
        <h2>コントローラー設定</h2>
        <div className="controller-settings">
          <select
            value={controllerType}
            onChange={(e) => setControllerType(e.target.value as ControllerType)}
            disabled={isConnected}
          >
            <option value="xbox">Xbox Controller</option>
            <option value="dualshock4">DualShock 4</option>
          </select>
          {isConnected ? (
            <button onClick={handleDisconnect}>切断</button>
          ) : (
            <button onClick={handleConnect}>接続</button>
          )}
        </div>
        {connectionMessage && <p className="message">{connectionMessage}</p>}
      </section>

      {/* Playback Controls */}
      {isConnected && (
        <>
          <section className="section">
            <h2>再生制御</h2>
            <div className="playback-controls">
              <button onClick={handleLoadFile}>CSVファイルを読み込む</button>
              {!isPlaying ? (
                <>
                  <button onClick={handleStartPlayback} disabled={totalFrames === 0}>
                    再生開始
                  </button>
                  <button onClick={handleResumePlayback}>再開</button>
                </>
              ) : (
                <button onClick={handlePausePlayback}>一時停止</button>
              )}
              <button onClick={handleStopPlayback}>停止</button>
            </div>
            <div className="playback-info">
              <p>
                進行状況: {currentFrame} / {totalFrames} フレーム
              </p>
              <progress value={currentFrame} max={totalFrames || 1} />
            </div>
            <div className="invert-control">
              <label>
                <input
                  type="checkbox"
                  checked={invertHorizontal}
                  onChange={(e) => handleInvertToggle(e.target.checked)}
                />
                左右反転
              </label>
            </div>
          </section>

          {/* Manual Input */}
          <section className="section">
            <h2>手動入力</h2>
            <div className="manual-input">
              <div className="direction-pad">
                <h3>方向パッド</h3>
                <div className="dpad-grid">
                  {directionPad.map((dir) => (
                    <button
                      key={dir.value}
                      className={direction === dir.value ? "active" : ""}
                      onClick={() => handleDirectionChange(dir.value)}
                    >
                      {dir.label}
                    </button>
                  ))}
                </div>
              </div>
              <div className="buttons">
                <h3>ボタン</h3>
                <div className="button-grid">
                  {Object.keys(buttonStates).map((buttonName) => (
                    <button
                      key={buttonName}
                      className={buttonStates[buttonName] === 1 ? "active" : ""}
                      onClick={() => handleButtonToggle(buttonName)}
                    >
                      {buttonName}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </section>
        </>
      )}
    </main>
  );
}

export default App;
