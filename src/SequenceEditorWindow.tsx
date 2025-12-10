import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import SequenceEditor from "./SequenceEditor";
import { api } from "./api";
import "./SequenceEditor.css";

function SequenceEditorWindow() {
  const [csvPath, setCsvPath] = useState<string | null>(null);
  const [currentPlayingRow, setCurrentPlayingRow] = useState<number>(-1);
  const [sequenceButtons, setSequenceButtons] = useState<string[]>([]);

  useEffect(() => {
    console.log("========== SequenceEditorWindow mounted ==========");
    console.log("Window location:", window.location.href);
    
    // URLパラメータからCSVパスを取得
    const params = new URLSearchParams(window.location.search);
    const encodedPath = params.get('csvPath');
    console.log("URL search params:", window.location.search);
    console.log("Encoded CSV path:", encodedPath);
    
    if (encodedPath) {
      try {
        // Base64デコード
        const decodedPath = atob(encodedPath);
        console.log("✓ Decoded CSV path:", decodedPath);
        setCsvPath(decodedPath);
      } catch (error) {
        console.error("✗ Failed to decode CSV path:", error);
      }
    } else {
      console.warn("✗ No csvPath parameter found in URL");
    }
    
    // マッピングを読み込んでsequenceButtonsを取得
    const loadMapping = async () => {
      try {
        const mapping = await api.loadButtonMapping("config/button_mapping.json");
        const csvButtons = mapping.mapping.map(btn => btn.user_button);
        const seqButtons = mapping.mapping
          .filter(btn => btn.use_in_sequence)
          .map(btn => btn.user_button);
        if (seqButtons.length > 0) {
          setSequenceButtons(seqButtons);
        } else {
          setSequenceButtons(csvButtons);
        }
      } catch (error) {
        console.log("マッピング読み込みエラー:", error);
        setSequenceButtons([]);
      }
    };
    
    loadMapping();
  }, []);

  // 再生中のフレーム番号を定期的に取得
  useEffect(() => {
    if (csvPath) {
      console.log("Starting playback frame polling for:", csvPath);
    }
    
    const interval = setInterval(async () => {
      if (csvPath) {
        try {
          const playingFrame = await api.getCurrentPlayingFrame();
          if (playingFrame !== currentPlayingRow) {
            console.log("Current playing frame:", playingFrame);
          }
          setCurrentPlayingRow(playingFrame);
        } catch (error) {
          // エラーは無視（再生中でない場合など）
        }
      }
    }, 100);

    return () => {
      console.log("Stopping playback frame polling");
      clearInterval(interval);
    };
  }, [csvPath, currentPlayingRow]);

  const handleClose = () => {
    console.log("Close button clicked");
    getCurrentWindow().close();
  };

  console.log("========== Rendering SequenceEditorWindow ==========");
  console.log("csvPath:", csvPath);
  console.log("currentPlayingRow:", currentPlayingRow);

  if (!csvPath) {
    console.log("Showing loading screen...");
    const params = new URLSearchParams(window.location.search);
    return (
      <div style={{ 
        display: "flex", 
        flexDirection: "column",
        justifyContent: "center", 
        alignItems: "center", 
        height: "100vh",
        width: "100vw",
        background: "#1e1e1e",
        color: "#fff",
        fontSize: "18px",
        padding: "20px",
        boxSizing: "border-box"
      }}>
        <div style={{ textAlign: "center", maxWidth: "600px" }}>
          <h2 style={{ color: "#0f0" }}>Sequence Editor</h2>
          <p>読み込み中...</p>
          <p style={{ fontSize: "14px", color: "#888" }}>CSVパスを待機しています</p>
          <div style={{ 
            marginTop: "20px", 
            padding: "10px", 
            background: "rgba(255,255,255,0.05)",
            borderRadius: "4px",
            fontSize: "12px",
            textAlign: "left",
            wordBreak: "break-all"
          }}>
            <p><strong>URL:</strong> {window.location.href}</p>
            <p><strong>Search:</strong> {window.location.search || "(empty)"}</p>
            <p><strong>csvPath param:</strong> {params.get('csvPath') || "(not found)"}</p>
          </div>
        </div>
      </div>
    );
  }

  console.log("Rendering SequenceEditor component");
  return (
    <div style={{ width: "100%", height: "100vh" }}>
      <SequenceEditor
        csvPath={csvPath}
        onClose={handleClose}
        currentPlayingRow={currentPlayingRow}
        sequenceButtons={sequenceButtons}
      />
    </div>
  );
}

export default SequenceEditorWindow;
