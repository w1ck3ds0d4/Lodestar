import { useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize, type PhysicalSize } from "@tauri-apps/api/dpi";
import { Camera, ChevronDown, ClipboardPaste, Crop, Ear, MessageSquare, Minus, ScanSearch, Send, Sparkles, Square, X } from "lucide-react";
import type { CaptureState } from "../types";
import { Answer } from "./MarkdownAnswer";
import { sendCaptureCommand as send, usePersistedBounds } from "../lib/captureWindow";

export type PanelKind = "answers" | "transcript" | "controls";

const EMPTY: CaptureState = { listening: false, watching: false, segments: [], answers: [] };

// Borderless windows have no native resize edges, so drive a resize from a corner grip.
const startResize = () => void getCurrentWindow().startResizeDragging("SouthEast");

// One floating overlay window. The whole panel is its own borderless window; the
// header is a drag region so it can be moved anywhere over a call. It is a thin
// view: state arrives over `capture-state`, actions go back over `capture-command`.
export function CapturePanel({ kind }: { kind: PanelKind }) {
  const [st, setSt] = useState<CaptureState>(EMPTY);
  const [draft, setDraft] = useState("");
  // Overlay translucency, applied to every capture window together (Controls panel only).
  const [opacity, setOpacity] = useState(1);
  // Collapsed = shrunk to a small icon button; click the chevron to expand again.
  const [collapsed, setCollapsed] = useState(false);
  const collapsedRef = useRef(false);
  const expandedSize = useRef<PhysicalSize | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  usePersistedBounds(collapsedRef);

  // Collapse the panel to a small icon (remembering its size) or expand it back.
  const toggleCollapsed = async () => {
    const win = getCurrentWindow();
    if (!collapsed) {
      try {
        expandedSize.current = await win.innerSize();
      } catch {
        /* ignore */
      }
      collapsedRef.current = true;
      setCollapsed(true);
      try {
        await win.setSize(new LogicalSize(116, 44));
      } catch {
        /* ignore */
      }
    } else {
      collapsedRef.current = false;
      setCollapsed(false);
      try {
        if (expandedSize.current) await win.setSize(expandedSize.current);
      } catch {
        /* ignore */
      }
    }
  };

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
    <div className={"cap-panel cap-" + kind + (collapsed ? " collapsed" : "")}>
      <div className="cap-head" data-tauri-drag-region>
        <span className="cap-title" title={collapsed ? meta.title : undefined}>
          <meta.Icon size={13} />
          {!collapsed && meta.title}
        </span>
        {!collapsed && (
          <span className={"cap-dot" + (st.listening ? " on" : "")} title={st.listening ? "Listening" : "Idle"} />
        )}
        <button
          className="cap-x cap-min"
          title={collapsed ? "Expand panel" : "Collapse to an icon"}
          onClick={() => void toggleCollapsed()}
        >
          {collapsed ? <ChevronDown size={14} /> : <Minus size={14} />}
        </button>
        {!collapsed && (
          <button className="cap-x" title="Exit capture mode" onClick={() => send({ cmd: "stop" })}>
            <X size={14} />
          </button>
        )}
      </div>

      {!collapsed && (
      <>
      <div className="cap-body" ref={bodyRef}>
        {kind === "answers" &&
          (st.answers.length === 0 ? (
            <div className="cap-empty">No answers yet.</div>
          ) : (
            st.answers.map((a, i) => (
              <div key={i} className="cap-qa">
                <div className="cap-q">{a.q}</div>
                {a.pending && !a.a ? (
                  <div className="cap-a muted">Thinking...</div>
                ) : (
                  <Answer text={a.a} />
                )}
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
            <div className="cap-btns">
              <button
                className="cap-btn"
                onClick={() => send({ cmd: "answer_screen" })}
                title="Answer whatever question/problem is on the screen right now"
              >
                <Camera size={14} /> Answer screen
              </button>
              <button
                className="cap-btn"
                onClick={() => send({ cmd: "read_page" })}
                title="Read the page you copied (Ctrl+A then Ctrl+C in the exercise) as context - captures the whole page, including off-screen text"
              >
                <ClipboardPaste size={14} /> Read page
              </button>
            </div>
            <label className="cap-opacity">
              <span>Opacity</span>
              <input
                type="range"
                min={0.3}
                max={1}
                step={0.05}
                value={opacity}
                title="Dim the whole overlay so it sits discreetly over your call"
                onChange={(e) => {
                  const v = Number(e.target.value);
                  setOpacity(v);
                  send({ cmd: "opacity", value: v });
                }}
              />
              <span className="cap-opacity-val">{Math.round(opacity * 100)}%</span>
            </label>
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
      </>
      )}
    </div>
  );
}
