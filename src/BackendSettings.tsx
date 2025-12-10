import { useState } from "react";
import "./BackendSettings.css";

interface BackendSettingsProps {
  currentBackend: "cpu" | "wgpu";
  onBackendChange: (backend: "cpu" | "wgpu") => void;
  onClose: () => void;
}

function BackendSettings({ currentBackend, onBackendChange, onClose }: BackendSettingsProps) {
  const [selectedBackend, setSelectedBackend] = useState<"cpu" | "wgpu">(currentBackend);

  const handleSave = () => {
    onBackendChange(selectedBackend);
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="backend-settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>バックエンド設定</h2>
          <button className="close-button" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="modal-body">
          <div className="backend-description">
            <p>モデル学習時に使用する計算バックエンドを選択してください。</p>
          </div>

          <div className="backend-options">
            <label className={`backend-option ${selectedBackend === "cpu" ? "selected" : ""}`}>
              <input
                type="radio"
                name="backend"
                value="cpu"
                checked={selectedBackend === "cpu"}
                onChange={() => setSelectedBackend("cpu")}
              />
              <div className="option-content">
                <div className="option-title">🖥️ CPU</div>
                <div className="option-description">
                  CPUを使用して学習を行います。
                  <br />
                  どの環境でも動作しますが、処理速度は遅くなります。
                </div>
              </div>
            </label>

            <label className={`backend-option ${selectedBackend === "wgpu" ? "selected" : ""}`}>
              <input
                type="radio"
                name="backend"
                value="wgpu"
                checked={selectedBackend === "wgpu"}
                onChange={() => setSelectedBackend("wgpu")}
              />
              <div className="option-content">
                <div className="option-title">⚡ GPU (WGPU)</div>
                <div className="option-description">
                  GPUを使用して学習を行います。
                  <br />
                  対応GPUが必要ですが、高速に処理できます。
                </div>
              </div>
            </label>
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn-cancel" onClick={onClose}>
            キャンセル
          </button>
          <button className="btn-save" onClick={handleSave}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

export default BackendSettings;
