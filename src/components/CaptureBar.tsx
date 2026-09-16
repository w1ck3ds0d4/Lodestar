import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "../lib/tauriEvent";
import { Camera, Ear, Eye, EyeOff, ScreenShare, ScreenShareOff, Square, X } from "lucide-react";
import * as api from "../api";
import type { CaptureState } from "../types";
import { sendCaptureCommand as send, usePersistedBounds } from "../lib/captureWindow";

const EMPTY: CaptureState = { listening: false, watching: false, segments: [], answers: [] };

// The always-on-top control bar docked at the top of the screen. It is the home for
// the quick controls during a call: start/stop capturing, screenshot the screen, and
// show/hide the panels. The whole strip is a drag region so it can be repositioned.
export function CaptureBar() {
  const [st, setSt] = useState<CaptureState>(EMPTY);
  const [panelsShown, setPanelsShown] = useState(true);
  usePersistedBounds();

  useEffect(() => {
    let cancelled = false;
    let un: UnlistenFn | null = null;
    void listen<CaptureState>("capture-state", (e) => setSt(e.payload)).then((u) =>
      cancelled ? u() : (un = u),
    );
    send({ cmd: "sync" });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  const togglePanels = () => {
    void api.toggleCapturePanels().then(setPanelsShown);
  };

  return (
    <div className="cap-bar" data-tauri-drag-region>
      <button
        className={"cap-bar-btn" + (st.listening ? " on" : "")}
        onClick={() => send({ cmd: "listen" })}
        title={st.listening ? "Stop capturing" : "Start capturing"}
      >
        {st.listening ? <Square size={15} /> : <Ear size={15} />}
      </button>
      <button
        className="cap-bar-btn"
        onClick={() => send({ cmd: "answer_screen" })}
        title="Answer the screen now"
      >
        <Camera size={15} />
      </button>
      <button
        className="cap-bar-btn"
        onClick={togglePanels}
        title={panelsShown ? "Hide panels (from you too)" : "Show panels"}
      >
        {panelsShown ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
      <button
        className={"cap-bar-btn" + (st.protect === false ? " on" : "")}
        onClick={() => send({ cmd: "protect" })}
        title={
          st.protect === false
            ? "Panels are visible to screenshots / screen share - click to hide them from capture"
            : "Show panels in screenshots / screen share (so you can capture them)"
        }
      >
        {st.protect === false ? <ScreenShare size={15} /> : <ScreenShareOff size={15} />}
      </button>
      <span className="cap-bar-sep" />
      <button className="cap-bar-btn danger" onClick={() => send({ cmd: "stop" })} title="Exit capture mode">
        <X size={15} />
      </button>
    </div>
  );
}
