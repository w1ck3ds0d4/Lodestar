import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";

// Custom window chrome for the borderless main window (decorations are off so the stealth
// "Hide from taskbar & Alt+Tab" can use the reliable tool-window flip without ever showing a
// native frame). The top bar acts as the title bar (drag region); these add the window buttons
// and invisible edge handles that restore native-feeling resize.

// Minimize / maximize-restore / close, rendered at the right of the top bar.
export function WindowControls() {
  // Track the maximized state so the middle button shows the right icon/label.
  const [maxed, setMaxed] = useState(false);
  useEffect(() => {
    const win = getCurrentWindow();
    let un: (() => void) | undefined;
    const sync = () =>
      void win
        .isMaximized()
        .then(setMaxed)
        .catch(() => {});
    sync();
    void win.onResized(sync).then((u) => (un = u));
    return () => un?.();
  }, []);

  return (
    <div className="win-controls">
      <button className="win-btn" title="Minimize" onClick={() => void getCurrentWindow().minimize()}>
        <Minus size={16} />
      </button>
      <button
        className="win-btn"
        title={maxed ? "Restore" : "Maximize"}
        onClick={() => void getCurrentWindow().toggleMaximize()}
      >
        {maxed ? <Copy size={13} /> : <Square size={13} />}
      </button>
      <button className="win-btn win-close" title="Close" onClick={() => void getCurrentWindow().close()}>
        <X size={16} />
      </button>
    </div>
  );
}

// 8 thin, invisible handles pinned to the window edges/corners; a borderless window loses
// native resize borders, so each one drives a resize in its direction via Tauri.
const EDGES = [
  ["n", "North"],
  ["s", "South"],
  ["e", "East"],
  ["w", "West"],
  ["ne", "NorthEast"],
  ["nw", "NorthWest"],
  ["se", "SouthEast"],
  ["sw", "SouthWest"],
] as const;

export function ResizeHandles() {
  return (
    <>
      {EDGES.map(([cls, dir]) => (
        <div
          key={cls}
          className={"rz rz-" + cls}
          onMouseDown={(e) => {
            if (e.button === 0) void getCurrentWindow().startResizeDragging(dir);
          }}
        />
      ))}
    </>
  );
}
