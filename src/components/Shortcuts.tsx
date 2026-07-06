import { Keyboard, X } from "lucide-react";

type Row = { keys: string[]; label: string };

// Global keyboard hotkeys (registered in src-tauri/src/lib.rs). All are Alt+Shift so they never
// collide with browser/app shortcuts, and they fire even while Lodestar is hidden.
const KEYBOARD: Row[] = [
  { keys: ["Alt", "Shift", "L"], label: "Listen to the call and transcribe both sides" },
  { keys: ["Alt", "Shift", "A"], label: "Answer what is on screen right now" },
  { keys: ["Alt", "Shift", "W"], label: "Watch the screen and re-answer as it changes" },
  { keys: ["Alt", "Shift", "R"], label: "Read the focused page via accessibility" },
  { keys: ["Alt", "Shift", "C"], label: "Show or hide the capture overlay panels" },
  { keys: ["Alt", "Shift", "H"], label: "Panic hide or show the window" },
  { keys: ["Alt", "Shift", "Up"], label: "Raise window opacity" },
  { keys: ["Alt", "Shift", "Down"], label: "Lower window opacity" },
];

// Mouse side-button gestures (src-tauri/src/mouse_hook.rs).
const MOUSE: Row[] = [
  { keys: ["MB4", "hold"], label: "Peek behind: hide while held, show on release" },
  { keys: ["MB5", "drag"], label: "Move the window" },
  { keys: ["MB5", "double-click"], label: "Hide or show the window" },
];

function Section({ title, rows }: { title: string; rows: Row[] }) {
  return (
    <div className="shortcuts-section">
      <strong>{title}</strong>
      <ul className="shortcuts-list">
        {rows.map((r) => (
          <li key={r.label}>
            <span className="shortcuts-keys">
              {r.keys.map((k) => (
                <span key={k} className="kbd">
                  {k}
                </span>
              ))}
            </span>
            <span className="muted sm">{r.label}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

// Reference sheet for every keyboard hotkey and mouse gesture. Opened from the top-bar keyboard
// icon - handy since the window is often hidden and the actions are driven by global shortcuts.
export function Shortcuts({ onClose }: { onClose: () => void }) {
  return (
    <div className="intro-overlay" onClick={onClose}>
      <div className="intro-card shortcuts-card" onClick={(e) => e.stopPropagation()}>
        <button className="icon-btn intro-close" aria-label="Close" onClick={onClose}>
          <X size={18} />
        </button>
        <h2 className="shortcuts-title">
          <Keyboard size={18} /> Shortcuts
        </h2>
        <p className="muted">Global shortcuts work anywhere, even while Lodestar is hidden.</p>
        <Section title="Keyboard" rows={KEYBOARD} />
        <Section title="Mouse (side buttons)" rows={MOUSE} />
      </div>
    </div>
  );
}
