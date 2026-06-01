import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { CapturePanel, type PanelKind } from "./components/CapturePanel";
import "./styles.css";

// Capture overlay windows reuse this same bundle; their window label (`cap-<kind>`)
// selects the lightweight panel view instead of the full app.
const label = getCurrentWindow().label;
const panelKind = label.startsWith("cap-") ? (label.slice(4) as PanelKind) : null;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{panelKind ? <CapturePanel kind={panelKind} /> : <App />}</React.StrictMode>,
);
