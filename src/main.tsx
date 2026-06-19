import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { CaptureBar } from "./components/CaptureBar";
import { CapturePanel, type PanelKind } from "./components/CapturePanel";
import "./styles.css";

// Capture overlay windows reuse this same bundle; the window label selects the view:
// `cap-bar` -> the control bar, `cap-<kind>` -> a panel, anything else -> the full app.
// In a plain browser (the Vite dev preview) there is no Tauri runtime, so
// getCurrentWindow() throws; fall back to the main-window view so the app mounts.
let label = "";
try {
  label = getCurrentWindow().label;
} catch {
  /* no Tauri runtime (browser preview): treat as the main window */
}
const isCapture = label.startsWith("cap-");

// The launch splash (in index.html) is only meaningful for the main window's first
// data load. Capture overlay windows render instantly and never run App's splash
// removal, so drop it right away for them - otherwise it covers the panels forever.
if (isCapture) {
  document.getElementById("jr-splash")?.remove();
}

function Root() {
  if (label === "cap-bar") return <CaptureBar />;
  if (isCapture) return <CapturePanel kind={label.slice(4) as PanelKind} />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
