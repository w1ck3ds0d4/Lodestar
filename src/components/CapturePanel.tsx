import { useEffect, useMemo, useRef, useState } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import { Crop, Ear, MessageSquare, ScanSearch, Send, Sparkles, Square, X } from "lucide-react";
import type { CaptureCommand, CaptureState } from "../types";

export type PanelKind = "answers" | "transcript" | "controls";

const EMPTY: CaptureState = { listening: false, watching: false, segments: [], answers: [] };

const send = (c: CaptureCommand) => void emit("capture-command", c);
// Borderless windows have no native resize edges, so drive a resize from a corner grip.
const startResize = () => void getCurrentWindow().startResizeDragging("SouthEast");

// Remember each panel's window bounds (per label) so the interview layout persists
// across sessions. Restores on open, then saves move/resize changes (debounced).
function usePersistedBounds() {
  useEffect(() => {
    const win = getCurrentWindow();
    const key = "cap.bounds." + win.label;
    const acc: { x?: number; y?: number; w?: number; h?: number } = {};
    try {
      const saved = JSON.parse(localStorage.getItem(key) || "{}");
      Object.assign(acc, saved);
      if (typeof acc.w === "number" && typeof acc.h === "number") {
        void win.setSize(new PhysicalSize(acc.w, acc.h));
      }
      if (typeof acc.x === "number" && typeof acc.y === "number") {
        void win.setPosition(new PhysicalPosition(acc.x, acc.y));
      }
    } catch {
      /* ignore malformed bounds */
    }

    let t: number | undefined;
    const save = (patch: Partial<typeof acc>) => {
      Object.assign(acc, patch);
      window.clearTimeout(t);
      t = window.setTimeout(() => localStorage.setItem(key, JSON.stringify(acc)), 300);
    };
    let cancelled = false;
    const uns: UnlistenFn[] = [];
    const add = (p: Promise<UnlistenFn>) => void p.then((u) => (cancelled ? u() : uns.push(u)));
    add(win.onMoved(({ payload }) => save({ x: payload.x, y: payload.y })));
    add(win.onResized(({ payload }) => save({ w: payload.width, h: payload.height })));
    return () => {
      cancelled = true;
      window.clearTimeout(t);
      uns.forEach((u) => u());
    };
  }, []);
}

// One floating overlay window. The whole panel is its own borderless window; the
// header is a drag region so it can be moved anywhere over a call. It is a thin
// view: state arrives over `capture-state`, actions go back over `capture-command`.
export function CapturePanel({ kind }: { kind: PanelKind }) {
  const [st, setSt] = useState<CaptureState>(EMPTY);
  const [draft, setDraft] = useState("");
  const bodyRef = useRef<HTMLDivElement>(null);
  usePersistedBounds();

  useEffect(() => {
    let cancelled = false;
    let un: UnlistenFn | null = null;
    void listen<CaptureState>("capture-state", (e) => setSt(e.payload)).then((u) =>
      cancelled ? u() : (un = u),
    );
    // Ask the main window to push the current state now (this window just opened).
    send({ cmd: "sync" });
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  // Keep the transcript pinned to the newest line as it grows.
  useEffect(() => {
    if (kind !== "answers") bodyRef.current?.scrollTo({ top: bodyRef.current.scrollHeight });
  }, [st.segments, kind]);

  const meta = useMemo(() => {
    if (kind === "answers") return { title: "Answers", Icon: Sparkles };
    if (kind === "transcript") return { title: "Transcript", Icon: MessageSquare };
    return { title: "Controls", Icon: ScanSearch };
  }, [kind]);

  const submitAsk = () => {
    const q = draft.trim();
    if (!q) return;
    send({ cmd: "ask", text: q });
    setDraft("");
  };

  return (
    <div className={"cap-panel cap-" + kind}>
      <div className="cap-head" data-tauri-drag-region>
        <span className="cap-title">
          <meta.Icon size={13} /> {meta.title}
        </span>
        <span className={"cap-dot" + (st.listening ? " on" : "")} title={st.listening ? "Listening" : "Idle"} />
        <button className="cap-x" title="Exit capture mode" onClick={() => send({ cmd: "stop" })}>
          <X size={14} />
        </button>
      </div>

      <div className="cap-body" ref={bodyRef}>
        {kind === "answers" &&
          (st.answers.length === 0 ? (
            <div className="cap-empty">No answers yet.</div>
          ) : (
            st.answers.map((a, i) => (
              <div key={i} className="cap-qa">
                <div className="cap-q">{a.q}</div>
                <div className="cap-a">{a.pending && !a.a ? "…" : a.a}</div>
              </div>
            ))
          ))}

        {kind === "transcript" &&
          (st.segments.length === 0 ? (
            <div className="cap-empty">No conversation yet.</div>
          ) : (
            st.segments.map((s, i) => (
              <div key={i} className={"cap-seg" + (s.role === "you" ? " you" : "")}>
                <span className="cap-who">{s.speaker}</span>
                <span className="cap-txt">{s.text}</span>
              </div>
            ))
          ))}

        {kind === "controls" && (
          <div className="cap-controls">
            <div className="cap-btns">
              <button className={"cap-btn" + (st.listening ? " on" : "")} onClick={() => send({ cmd: "listen" })}>
                {st.listening ? <Square size={14} /> : <Ear size={14} />}
                {st.listening ? "Stop" : "Listen"}
              </button>
              <button
                className={"cap-btn" + (st.watching ? " on" : "")}
                onClick={() => send({ cmd: "watch" })}
                title="Continuously read the screen and answer what it shows"
              >
                <ScanSearch size={14} />
                {st.watching ? "Watching" : "Watch screen"}
              </button>
              <button className="cap-btn" onClick={() => send({ cmd: "snip" })} title="Snip a region and explain it">
                <Crop size={14} /> Snip
              </button>
            </div>
            <div className="cap-ask">
              <input
                value={draft}
                placeholder="Ask a question…"
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") submitAsk();
                }}
              />
              <button className="cap-btn" onClick={submitAsk} aria-label="Ask">
                <Send size={14} />
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="cap-grip" onMouseDown={startResize} title="Drag to resize" />
    </div>
  );
}
