import React from "react";
import ReactDOM from "react-dom/client";
import SequenceEditorWindow from "./SequenceEditorWindow";
import "./App.css";

console.log("========== Editor main script loading ==========");

// デバッグ情報を更新
const updateDebugInfo = (text: string) => {
  const debugEl = document.getElementById("debug-info");
  if (debugEl) {
    debugEl.textContent = text;
  }
};

updateDebugInfo("editor-main.tsx loaded");

const rootElement = document.getElementById("editor-root");
console.log("Root element:", rootElement);

if (rootElement) {
  updateDebugInfo("Mounting React app...");
  try {
    ReactDOM.createRoot(rootElement).render(
      <React.StrictMode>
        <SequenceEditorWindow />
      </React.StrictMode>
    );
    console.log("✓ Editor app mounted");
    updateDebugInfo("React app mounted");
  } catch (error) {
    console.error("✗ Failed to mount React app:", error);
    updateDebugInfo("Error: " + error);
  }
} else {
  console.error("✗ Failed to find editor-root element");
  updateDebugInfo("Error: editor-root not found");
}
