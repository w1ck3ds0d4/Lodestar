import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { CaptureBar } from "./components/CaptureBar";
import { CapturePanel, type PanelKind } from "./components/CapturePanel";
import "./styles.css";

// Capture overlay windows reuse this same bundle; the window label selects the view:
// `cap-bar` -> the control bar, `cap-<kind>` -> a panel, anything else -> the full app.
const label = getCurrentWindow().label;

function Root() {
  if (label === "cap-bar") return <CaptureBar />;
  if (label.startsWith("cap-")) return <CapturePanel kind={label.slice(4) as PanelKind} />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
