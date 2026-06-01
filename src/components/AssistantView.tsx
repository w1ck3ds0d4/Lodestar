import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Channel } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Bot,
  ClipboardPaste,
  Crop,
  Ear,
  KeyRound,
  MessageSquare,
  Monitor,
  ScanSearch,
  Send,
  SlidersHorizontal,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import * as api from "../api";
import {
  ANSWER_STYLES,
  ASSISTANT_MODELS,
  DEEPGRAM_PER_MIN,
  STT_LANGUAGES,
  estimateCost,
  type AudioDevices,
  type DeepgramBalance,
  type MonitorInfo,
  type UsageReport,
} from "../types";
import { ApplicationKit } from "./ApplicationKit";
import { SessionsPanel } from "./SessionsPanel";
import { StoriesPanel } from "./StoriesPanel";

interface QA {
  id: number;
  q: string;
  a: string;
  pending: boolean;
  error?: string;
}

// One captured audio source reported by the backend for the diagnostics panel.
interface CaptureDevice {
  role: string;
  name: string;
  rate: number;
  channels: number;
}

// One attributed line of the live dialog: who said it and what.
interface DialogSegment {
  id: number;
  role: "you" | "them";
  speaker: string;
  text: string;
}

// Stable colour bucket for a speaker name (so each speaker keeps one chip hue).
function chipClass(speaker: string): string {
  let h = 0;
  for (const c of speaker) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return "seg-chip--c" + (h % 5);
}

function Answer({ text }: { text: string }) {
  return (
    <div className="assistant-answer">
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

// True if an answer contains a fenced code block or LaTeX math.
function hasCodeOrMath(a: string): boolean {
  return a.includes("```") || a.includes("$$") || /\$[^$\n]+\$/.test(a);
}

interface SnippetGroup {
  id: number;
  q: string;
  items: { key: string; md: string }[];
}

// Pull the fenced code blocks + display-math out of each answer, grouped by the
// question they came from (newest group first), so the Code & Math pane is a tidy
// per-question reference rather than a duplicate of the full answer.
function snippetGroups(qa: QA[]): SnippetGroup[] {
  const groups: SnippetGroup[] = [];
  for (const e of qa) {
    if (e.pending || e.error || !e.a || !hasCodeOrMath(e.a)) continue;
    const items: { key: string; md: string }[] = [];
    let m: RegExpExecArray | null;
    const codeRe = /```(\w*)\n?([\s\S]*?)```/g;
    let i = 0;
    while ((m = codeRe.exec(e.a))) {
      const body = m[2].replace(/\n+$/, "");
      if (body.trim()) items.push({ key: `${e.id}-c${i++}`, md: "```" + (m[1] || "") + "\n" + body + "\n```" });
    }
    const mathRe = /\$\$([\s\S]*?)\$\$/g;
    let j = 0;
    while ((m = mathRe.exec(e.a))) {
      const body = m[1].trim();
      if (body) items.push({ key: `${e.id}-m${j++}`, md: "$$" + body + "$$" });
    }
    if (items.length) groups.push({ id: e.id, q: e.q, items });
  }
  return groups;
}

// Fullscreen "snip & explain": shows a captured screenshot, drag to select a
// region, and crops it client-side to a base64 PNG for the vision call.
function SnipOverlay({
  src,
  onCancel,
  onSelect,
}: {
  src: string;
  onCancel: () => void;
  onSelect: (base64: string) => void;
}) {
  const imgRef = useRef<HTMLImageElement>(null);
  const [start, setStart] = useState<{ x: number; y: number } | null>(null);
  const [cur, setCur] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  const rectStyle: CSSProperties | undefined =
    start && cur
      ? {
          left: Math.min(start.x, cur.x),
          top: Math.min(start.y, cur.y),
          width: Math.abs(cur.x - start.x),
          height: Math.abs(cur.y - start.y),
        }
      : undefined;

  const finish = () => {
    const img = imgRef.current;
    if (!img || !start || !cur) {
      setStart(null);
      setCur(null);
      return;
    }
    const r = img.getBoundingClientRect();
    const fx = img.naturalWidth / r.width;
    const fy = img.naturalHeight / r.height;
    const sx = (Math.min(start.x, cur.x) - r.left) * fx;
    const sy = (Math.min(start.y, cur.y) - r.top) * fy;
    const sw = Math.abs(cur.x - start.x) * fx;
    const sh = Math.abs(cur.y - start.y) * fy;
    setStart(null);
    setCur(null);
    if (sw < 8 || sh < 8) return; // ignore stray clicks / tiny selections
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(sw);
    canvas.height = Math.round(sh);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, canvas.width, canvas.height);
    onSelect(canvas.toDataURL("image/png").split(",")[1]);
  };

  return (
    <div className="snip-overlay" onMouseUp={finish}>
      <div className="snip-hint">Drag to select a region to explain · Esc to cancel</div>
      <div
        className="snip-stage"
        onMouseDown={(e) => {
          setStart({ x: e.clientX, y: e.clientY });
          setCur({ x: e.clientX, y: e.clientY });
        }}
        onMouseMove={(e) => start && setCur({ x: e.clientX, y: e.clientY })}
      >
        <img ref={imgRef} className="snip-img" src={src} draggable={false} alt="screen capture" />
      </div>
      {rectStyle && <div className="snip-rect" style={rectStyle} />}
      <button className="btn-ghost snip-cancel" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
}

function QaCard({ e }: { e: QA }) {
  return (
    <div className="qa">
      <div className="qa-q">{e.q}</div>
      {e.error ? (
        <div className="assistant-error">{e.error}</div>
      ) : e.pending && !e.a ? (
        <div className="muted sm">Thinking...</div>
      ) : (
        <Answer text={e.a} />
      )}
    </div>
  );
}

export function AssistantView() {
  const [loaded, setLoaded] = useState(false);
  // Which panel is shown when the window is too narrow for all three columns; the
  // tab bar that switches this only appears below the responsive breakpoint (CSS).
  // In the narrow layout the main view tabs between Dialog and Answers; Setup is a
  // slide-in drawer (settingsOpen) rather than a pane, so the live view stays put.
  const [pane, setPane] = useState<"dialog" | "answers">("dialog");
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Whether the narrow (tabbed) layout is active, and how many dialog/answer items
  // the user has already seen - so the Dialog/Answers tabs can show an unread count.
  const [narrow, setNarrow] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(max-width: 1500px)").matches,
  );
  const [seenDialog, setSeenDialog] = useState(0);
  const [seenAnswers, setSeenAnswers] = useState(0);
  const [hasKey, setHasKey] = useState(false);
  const [hasSttKey, setHasSttKey] = useState(false);
  const [sttEngine, setSttEngine] = useState("deepgram");
  const [sttLanguage, setSttLanguage] = useState("multi");
  const [sttDiarize, setSttDiarize] = useState(false);
  const [captureSource, setCaptureSource] = useState("system");
  const [devices, setDevices] = useState<AudioDevices>({ inputs: [], outputs: [] });
  const [inputDevice, setInputDevice] = useState("");
  const [outputDevice, setOutputDevice] = useState("");
  const [model, setModel] = useState("claude-opus-4-8");
  const [answerEngine, setAnswerEngine] = useState("claude");
  const [answerStyle, setAnswerStyle] = useState("auto");
  const [ollamaModel, setOllamaModel] = useState("");
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [keyInput, setKeyInput] = useState("");
  const [sttKeyInput, setSttKeyInput] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [savingStt, setSavingStt] = useState(false);

  const [listening, setListening] = useState(false);
  const [listenStatus, setListenStatus] = useState("");
  const [segments, setSegments] = useState<DialogSegment[]>([]);
  // Live transcription, accumulated per source role into one bubble per utterance.
  // `fin` is finalized-but-still-open text (won't change); `int` is the interim tail
  // that keeps being replaced as Deepgram refines it. The two are shown together as a
  // single growing line; on speech-final the whole thing commits into `segments`.
  // liveRef is the synchronous source of truth; `live` mirrors it for rendering.
  const emptyLive = () => ({ you: { fin: "", int: "" }, them: { fin: "", int: "" } });
  const liveRef = useRef(emptyLive());
  const [live, setLive] = useState(emptyLive());
  const [listenError, setListenError] = useState("");
  const [autoAnswer, setAutoAnswer] = useState(true);
  const [captureInfo, setCaptureInfo] = useState<CaptureDevice[]>([]);
  const [level, setLevel] = useState<{ peak: number; buffered: number }>({ peak: 0, buffered: 0 });

  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [screenMonitor, setScreenMonitor] = useState(0);
  const [watchingScreen, setWatchingScreen] = useState(false);
  const [screenError, setScreenError] = useState("");
  const [snipSrc, setSnipSrc] = useState<string | null>(null);
  const [pageMsg, setPageMsg] = useState("");
  const [sessionsReload, setSessionsReload] = useState(0);
  const [sessionSaved, setSessionSaved] = useState(false);
  const screenBusy = useRef(false);

  const [opacity, setOpacity] = useState(1);
  const [onTop, setOnTop] = useState(false);

  const [contextRole, setContextRole] = useState("");
  const [contextCompany, setContextCompany] = useState("");
  const [contextResume, setContextResume] = useState("");
  const [contextNotes, setContextNotes] = useState("");
  const [contextSaved, setContextSaved] = useState(false);

  const [usage, setUsage] = useState<UsageReport | null>(null);
  const [claudeCreditInput, setClaudeCreditInput] = useState("");
  const [dgCreditInput, setDgCreditInput] = useState("");
  const [dgBalance, setDgBalance] = useState<DeepgramBalance | null>(null);
  const [dgError, setDgError] = useState("");
  const [dgLoading, setDgLoading] = useState(false);

  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [qa, setQa] = useState<QA[]>([]);

  const idRef = useRef(0);
  const segIdRef = useRef(0);
  const answeredIdx = useRef(0);
  // Synchronous mirror of `context` so the hotkey event listener and Read page can
  // dedupe/append without a stale closure.
  const contextRef = useRef("");
  useEffect(() => {
    contextRef.current = context;
  }, [context]);

  // Append a captured chunk (clipboard or focused-window text) to the assistant's
  // context, de-duplicating repeats and capping the total so it stays affordable.
  const addToContext = useCallback((raw: string, label: string) => {
    const text = raw.trim();
    if (!text) {
      setPageMsg(`${label}: nothing captured.`);
      window.setTimeout(() => setPageMsg(""), 3500);
      return;
    }
    const prev = contextRef.current;
    if (prev.includes(text)) {
      setPageMsg(`${label}: already attached. Copy another part (e.g. the code editor) to add more.`);
      window.setTimeout(() => setPageMsg(""), 3500);
      return;
    }
    let next = prev ? prev + "\n\n----- next section -----\n\n" + text : text;
    let note = `${label}: attached ${text.length.toLocaleString()} chars as context.`;
    if (next.length > 40000) {
      next = next.slice(0, 40000) + "\n\n[...truncated]";
      note += " (context capped at 40k)";
    }
    contextRef.current = next;
    setContext(next);
    setPageMsg(note);
    window.setTimeout(() => setPageMsg(""), 3500);
  }, []);

  // Overlay window controls. Applied after mount (hwnd() errors before the window
  // exists); opacity is debounced so dragging the slider doesn't spam invoke.
  useEffect(() => {
    const t = setTimeout(() => void api.setWindowOpacity(opacity).catch(() => {}), 60);
    return () => clearTimeout(t);
  }, [opacity]);
  useEffect(() => {
    void api.setWindowAlwaysOnTop(onTop).catch(() => {});
  }, [onTop]);

  // Track the responsive breakpoint so unread badges only accrue when panes are
  // actually hidden behind tabs.
  useEffect(() => {
    const mq = window.matchMedia("(max-width: 1500px)");
    const on = () => {
      setNarrow(mq.matches);
      // In the wide layout Setup is always-visible, so close the drawer state.
      if (!mq.matches) setSettingsOpen(false);
    };
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  // A pane's items count as "seen" while its tab is showing (or in the wide layout
  // where everything is visible), which keeps that tab's badge cleared.
  useEffect(() => {
    if (!narrow || pane === "dialog") setSeenDialog(segments.length);
  }, [narrow, pane, segments.length]);
  useEffect(() => {
    if (!narrow || pane === "answers") setSeenAnswers(qa.length);
  }, [narrow, pane, qa.length]);

  // Build a speaker-prefixed transcript string from the dialog segments.
  const transcriptText = useCallback(
    (segs: DialogSegment[]) => segs.map((s) => `${s.speaker}: ${s.text}`).join("\n"),
    [],
  );

  const loadSettings = useCallback(async () => {
    try {
      const s = await api.getAssistantSettings();
      setHasKey(s.hasKey);
      setHasSttKey(s.hasSttKey);
      setSttEngine(s.sttEngine);
      setSttLanguage(s.sttLanguage || "multi");
      setSttDiarize(s.sttDiarize);
      setCaptureSource(s.captureSource);
      setInputDevice(s.inputDevice);
      setOutputDevice(s.outputDevice);
      setModel(s.model);
      setAnswerEngine(s.answerEngine);
      setAnswerStyle(s.answerStyle || "auto");
      setOllamaModel(s.ollamaModel);
      setContextRole(s.contextRole);
      setContextCompany(s.contextCompany);
      setContextResume(s.contextResume);
      setContextNotes(s.contextNotes);
    } finally {
      setLoaded(true);
    }
  }, []);

  const saveContext = async () => {
    await api.setInterviewContext(contextRole, contextCompany, contextResume, contextNotes);
    setContextSaved(true);
    setTimeout(() => setContextSaved(false), 1500);
  };
  const refreshOllamaModels = useCallback(() => {
    void api
      .listOllamaModels()
      .then(setOllamaModels)
      .catch(() => setOllamaModels([]));
  }, []);
  useEffect(() => {
    void loadSettings();
    void api.listAudioDevices().then(setDevices).catch(() => {});
    void api.listMonitors().then(setMonitors).catch(() => {});
    refreshOllamaModels();
  }, [loadSettings, refreshOllamaModels]);

  // Capture the chosen screen and append any answer to the Q&A feed. `force`
  // answers even an unchanged screen (manual button); auto-watch passes false so
  // a static screen costs nothing. A busy guard prevents overlapping calls.
  const runScreen = useCallback(
    async (force: boolean) => {
      if (screenBusy.current) return;
      screenBusy.current = true;
      try {
        const res = await api.answerScreen(screenMonitor, force);
        const a = res.answer.trim();
        if (res.changed && a && a.toUpperCase() !== "NONE") {
          setQa((prev) => [{ id: ++idRef.current, q: "From your screen", a, pending: false }, ...prev]);
        }
        setScreenError("");
      } catch (e) {
        setScreenError(String(e));
      } finally {
        screenBusy.current = false;
      }
    },
    [screenMonitor],
  );

  // Read page: pull whatever the user copied (Ctrl+A, Ctrl+C) from the clipboard and
  // attach it. Copies append (with dedupe) so multiple parts can be stacked - e.g. on a
  // coding page, the problem statement and the editor's code are separate widgets, so
  // copy each and click Read page for each. Clear (in the context note) resets.
  const readPage = useCallback(async () => {
    setPageMsg("");
    try {
      const text = await api.readClipboard();
      if (!text.trim()) {
        setPageMsg("Clipboard is empty - in your browser press Ctrl+A then Ctrl+C, then click Read page.");
        window.setTimeout(() => setPageMsg(""), 3500);
        return;
      }
      addToContext(text, "Read page");
    } catch (e) {
      setPageMsg("Couldn't read the clipboard: " + String(e));
    }
  }, [addToContext]);

  // Snip & explain: capture the chosen monitor, then crop a region to explain.
  const startSnip = useCallback(async () => {
    try {
      const b64 = await api.captureScreenPng(screenMonitor);
      setSnipSrc("data:image/png;base64," + b64);
      setScreenError("");
    } catch (e) {
      setScreenError(String(e));
    }
  }, [screenMonitor]);

  const onSnipSelect = useCallback((base64: string) => {
    setSnipSrc(null);
    const id = ++idRef.current;
    setQa((prev) => [{ id, q: "Screen snip", a: "", pending: true }, ...prev]);
    void (async () => {
      try {
        const a = await api.explainRegion(base64);
        setQa((prev) => prev.map((e) => (e.id === id ? { ...e, a, pending: false } : e)));
      } catch (e) {
        setQa((prev) => prev.map((e) => (e.id === id ? { ...e, error: String(e), pending: false } : e)));
      }
    })();
  }, []);

  // Auto-watch: poll the chosen screen while the toggle is on.
  useEffect(() => {
    if (!watchingScreen) return;
    void runScreen(false);
    const t = setInterval(() => void runScreen(false), 6000);
    return () => clearInterval(t);
  }, [watchingScreen, runScreen]);

  // Ask the assistant; the answer streams into its Q&A card (newest first).
  const ask = useCallback(async (q: string, ctx: string | null) => {
    const question = q.trim();
    if (!question) return;
    const id = ++idRef.current;
    setQa((prev) => [{ id, q: question, a: "", pending: true }, ...prev]);
    const channel = new Channel<api.AnswerChunk>();
    channel.onmessage = (msg) => {
      setQa((prev) =>
        prev.map((e) => {
          if (e.id !== id) return e;
          if (msg.kind === "delta") return { ...e, a: e.a + msg.text, pending: true };
          if (msg.kind === "done") return { ...e, a: msg.text || e.a, pending: false };
          return { ...e, error: msg.message, pending: false };
        }),
      );
    };
    try {
      await api.askAssistant(question, ctx && ctx.trim() ? ctx.trim() : null, channel);
    } catch (e) {
      setQa((prev) => prev.map((x) => (x.id === id ? { ...x, error: String(e), pending: false } : x)));
    }
  }, []);

  // Generate a company briefing + suggested interview questions from what the user
  // entered (saves the company first so every later answer is company-aware).
  const generateCompanyBrief = async () => {
    if (!contextCompany.trim()) return;
    await api.setInterviewContext(contextRole, contextCompany, contextResume, contextNotes);
    const forRole = contextRole.trim() ? ` for a ${contextRole.trim()} role` : "";
    void ask(
      `Brief me for an interview at this company:\n${contextCompany}\n\nCover concisely: what the company ` +
        `does and its products, its likely tech stack and engineering culture, and 6-8 specific, thoughtful ` +
        `questions I should ask the interviewer${forRole}. Note anything you are unsure about (you may not ` +
        `have current information about this company).`,
      null,
    );
  };

  // Live transcript + listen lifecycle events from the Rust capture loop. The
  // `cancelled` guard prevents duplicate listeners under React StrictMode: the async
  // listen() would otherwise resolve after cleanup and leak a second listener, so
  // every transcript event would be handled (and appended) twice.
  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];
    const add = (p: Promise<UnlistenFn>) =>
      void p.then((u) => (cancelled ? u() : unlisteners.push(u)));
    add(
      listen<{ role: "you" | "them"; speaker: string; text: string; final?: boolean; speechFinal?: boolean }>(
        "transcript",
        (e) => {
          const { role, speaker, text, final, speechFinal } = e.payload;
          const slot = liveRef.current[role];
          const sync = () =>
            setLive({ you: { ...liveRef.current.you }, them: { ...liveRef.current.them } });
          if (final === false) {
            // Interim refinement: replace the live tail (don't touch finalized text).
            slot.int = text;
            sync();
            return;
          }
          // Final span: append to the finalized text and clear the tail. Only on a
          // speech-final (the speaker paused) do we commit the whole utterance.
          if (text.trim()) slot.fin = slot.fin ? slot.fin + " " + text.trim() : text.trim();
          slot.int = "";
          if (speechFinal) {
            const acc = slot.fin.trim();
            if (acc) setSegments((s) => [...s, { id: ++segIdRef.current, role, speaker, text: acc }]);
            slot.fin = "";
          }
          sync();
        },
      ),
    );
    add(listen<string>("listen-status", (e) => setListenStatus(String(e.payload))));
    add(listen<string>("listen-error", (e) => setListenError(String(e.payload))));
    add(listen<CaptureDevice[]>("listen-device", (e) => setCaptureInfo(e.payload)));
    add(listen<{ peak: number; buffered: number }>("listen-level", (e) => setLevel(e.payload)));
    add(
      listen("listen-stopped", () => {
        // Flush any open live utterance so a sentence in progress isn't lost on stop.
        (["them", "you"] as const).forEach((role) => {
          const slot = liveRef.current[role];
          const acc = (slot.fin + " " + slot.int).trim();
          if (acc) {
            setSegments((s) => [
              ...s,
              { id: ++segIdRef.current, role, speaker: role === "them" ? "Them" : "You", text: acc },
            ]);
          }
        });
        liveRef.current = emptyLive();
        setLive(emptyLive());
        setListening(false);
        setListenStatus("");
        setCaptureInfo([]);
        setLevel({ peak: 0, buffered: 0 });
      }),
    );
    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
    };
  }, []);

  // Read-window hotkey (Alt+Shift+R): the Rust side reads the focused window via UI
  // Automation and pushes the text here. Used when a page blocks copy/paste.
  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];
    const add = (p: Promise<UnlistenFn>) => void p.then((u) => (cancelled ? u() : unlisteners.push(u)));
    add(listen<string>("window-text", (e) => addToContext(String(e.payload), "Read window")));
    add(
      listen<string>("window-text-error", (e) => {
        setPageMsg("Read window: " + String(e.payload));
        window.setTimeout(() => setPageMsg(""), 4000);
      }),
    );
    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
    };
  }, [addToContext]);

  // Auto-answer: send each batch of new dialog segments to the model, which answers
  // it if it is a question or problem (in any language) and replies "NONE" otherwise.
  // Keyed on segment count so newly attributed lines (you/them/Speaker N) are handled.
  useEffect(() => {
    if (!autoAnswer || !listening) return;
    const newSegs = segments.slice(answeredIdx.current);
    if (newSegs.length === 0) return;
    const latest = transcriptText(newSegs).trim();
    if (latest.length < 2) return;
    const ctx = transcriptText(segments.slice(0, answeredIdx.current));
    answeredIdx.current = segments.length;
    void (async () => {
      const id = ++idRef.current;
      let acc = "";
      let created = false;
      // Letters-only upper, so we can tell the "NONE" sentinel from a real answer even
      // through punctuation/whitespace, and hold the bubble until the stream diverges.
      const stripped = (s: string) => s.replace(/[^a-z]/gi, "").toUpperCase();
      const looksNone = (s: string) => "NONE".startsWith(stripped(s));
      const channel = new Channel<api.AnswerChunk>();
      channel.onmessage = (msg) => {
        if (msg.kind === "delta") {
          acc += msg.text;
          if (!created) {
            if (looksNone(acc)) return; // still ambiguous (N / NO / NON / NONE)
            created = true;
            setQa((prev) => [{ id, q: latest, a: acc, pending: true }, ...prev]);
          } else {
            setQa((prev) => prev.map((e) => (e.id === id ? { ...e, a: acc, pending: true } : e)));
          }
        } else if (msg.kind === "done") {
          const full = (msg.text || acc).trim();
          if (!full || stripped(full) === "NONE") {
            if (created) setQa((prev) => prev.filter((e) => e.id !== id)); // retract a NONE
            return;
          }
          if (!created) {
            created = true;
            setQa((prev) => [{ id, q: latest, a: full, pending: false }, ...prev]);
          } else {
            setQa((prev) => prev.map((e) => (e.id === id ? { ...e, a: full, pending: false } : e)));
          }
        } else if (created) {
          setQa((prev) => prev.map((e) => (e.id === id ? { ...e, error: msg.message, pending: false } : e)));
        }
        // delta/done errors before a bubble exists are dropped so the loop keeps going.
      };
      try {
        await api.autoAnswer(ctx, latest, channel);
      } catch {
        /* ignore transient auto-answer errors so the loop keeps going */
      }
    })();
  }, [segments, autoAnswer, listening, transcriptText]);

  const saveKey = async () => {
    if (!keyInput.trim()) return;
    setSavingKey(true);
    try {
      await api.setApiKey(keyInput.trim());
      setKeyInput("");
      await loadSettings();
    } finally {
      setSavingKey(false);
    }
  };

  const saveSttKey = async () => {
    if (!sttKeyInput.trim()) return;
    setSavingStt(true);
    try {
      await api.setDeepgramKey(sttKeyInput.trim());
      setSttKeyInput("");
      await loadSettings();
    } finally {
      setSavingStt(false);
    }
  };

  const changeModel = async (m: string) => {
    setModel(m);
    await api.setAssistantModel(m);
  };

  const changeAnswerEngine = async (e: string) => {
    setAnswerEngine(e);
    await api.setAnswerEngine(e);
    if (e === "ollama") refreshOllamaModels();
  };

  const changeStyle = async (st: string) => {
    setAnswerStyle(st);
    await api.setAnswerStyle(st);
  };

  const changeOllamaModel = async (m: string) => {
    setOllamaModel(m);
    await api.setOllamaModel(m);
  };

  const changeEngine = async (e: string) => {
    setSttEngine(e);
    await api.setSttEngine(e);
  };

  const changeCaptureSource = async (s: string) => {
    setCaptureSource(s);
    await api.setCaptureSource(s);
  };

  const changeLanguage = async (lang: string) => {
    setSttLanguage(lang);
    await api.setSttLanguage(lang);
  };

  const changeDiarize = async (on: boolean) => {
    setSttDiarize(on);
    await api.setSttDiarize(on);
  };

  // Lazily loaded when the Usage accordion is opened (and on Refresh).
  const refreshUsage = useCallback(async () => {
    void api
      .getUsage()
      .then((u) => {
        setUsage(u);
        setClaudeCreditInput(u.claudeCredit ? String(u.claudeCredit) : "");
        setDgCreditInput(u.deepgramCredit ? String(u.deepgramCredit) : "");
      })
      .catch(() => {});
    if (!hasSttKey) {
      setDgBalance(null);
      setDgError("");
      return;
    }
    setDgLoading(true);
    setDgError("");
    try {
      setDgBalance(await api.deepgramBalance());
    } catch (e) {
      setDgBalance(null);
      setDgError(String(e));
    } finally {
      setDgLoading(false);
    }
  }, [hasSttKey]);

  const resetUsage = async () => {
    await api.resetUsage();
    await refreshUsage();
  };

  const saveCredit = async (provider: "claude" | "deepgram", raw: string) => {
    const amount = Number.parseFloat(raw) || 0;
    await api.setUsageCredit(provider, amount);
    setUsage(await api.getUsage());
  };

  const claudeCost = (usage?.claudeItems ?? []).reduce((s, it) => s + estimateCost(it), 0);
  const claudeTokens = (usage?.claudeItems ?? []).reduce((s, it) => s + it.input + it.output, 0);
  const dgMinutes = (usage?.deepgramSeconds ?? 0) / 60;
  const dgCost = dgMinutes * DEEPGRAM_PER_MIN;
  const modelLabel = (id: string) => ASSISTANT_MODELS.find((m) => m.id === id)?.label ?? id;

  const changeOutputDevice = async (name: string) => {
    setOutputDevice(name);
    await api.setOutputDevice(name);
  };

  const changeInputDevice = async (name: string) => {
    setInputDevice(name);
    await api.setInputDevice(name);
  };

  const toggleListen = async () => {
    if (listening) {
      await api.stopListening();
      setListening(false);
      setListenStatus("");
      return;
    }
    setListenError("");
    liveRef.current = emptyLive();
    setLive(emptyLive());
    try {
      await api.startListening();
      setListening(true);
    } catch (e) {
      setListenError(String(e));
    }
  };

  const clearTranscript = () => {
    setSegments([]);
    liveRef.current = emptyLive();
    setLive(emptyLive());
    answeredIdx.current = 0;
  };

  // Persist the current transcript + answers as a reviewable session.
  const saveSession = async () => {
    const transcript = transcriptText(segments);
    const answers = qa
      .filter((e) => e.a && !e.pending)
      .slice()
      .reverse()
      .map((e) => `Q: ${e.q}\nA: ${e.a}`)
      .join("\n\n---\n\n");
    if (!transcript.trim() && !answers.trim()) return;
    try {
      await api.saveSession(`Session ${new Date().toLocaleString()}`, transcript, answers);
      setSessionsReload((n) => n + 1);
      setSessionSaved(true);
      setTimeout(() => setSessionSaved(false), 1800);
    } catch {
      /* ignore save errors */
    }
  };

  const askManual = () => {
    if (!question.trim()) return;
    void ask(question, context.trim() || transcriptText(segments));
    setQuestion("");
  };

  const needsDeepgramKey = sttEngine === "deepgram" && !hasSttKey;
  const cmGroups = useMemo(() => snippetGroups(qa), [qa]);
  const dialogUnread = Math.max(0, segments.length - seenDialog);
  const answersUnread = Math.max(0, qa.length - seenAnswers);
  const badge = (n: number) => (n > 9 ? "9+" : String(n));
  // Fill the opacity track exactly to the value (native accent-color leaves a gap at 100%).
  const opacityPct = Math.round(((opacity - 0.3) / 0.7) * 100);

  return (
    <div className="assistant three-col">
      <div className="assistant-bar">
        <div className="assistant-title">
          <Bot size={18} /> Assistant
        </div>
        <select value={model} onChange={(e) => void changeModel(e.target.value)} title="Answer model">
          {ASSISTANT_MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label} - {m.note}
            </option>
          ))}
        </select>
      </div>

      {!loaded ? (
        <div className="assistant-loading muted sm">Loading…</div>
      ) : !hasKey && answerEngine !== "ollama" ? (
        <div className="add-source">
          <h3>
            <KeyRound size={16} /> Connect your Anthropic API key
          </h3>
          <p className="muted sm">
            Stored locally on this machine only. Create a key at console.anthropic.com. When you ask, the
            assistant sends your question and any context to Anthropic to generate the answer.
          </p>
          <div className="add-row">
            <input
              type="password"
              placeholder="sk-ant-..."
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void saveKey();
              }}
            />
            <button className="btn-open" onClick={saveKey} disabled={savingKey || !keyInput.trim()}>
              {savingKey ? "Saving..." : "Save key"}
            </button>
          </div>
          <p className="muted sm">
            No key?{" "}
            <button className="link-btn" onClick={() => void changeAnswerEngine("ollama")}>
              Use a local model (Ollama) instead
            </button>{" "}
            - free and private, no API cost.
          </p>
        </div>
      ) : (
        <>
          {/* Shown only when the window is too narrow for all three columns (CSS).
              The gear opens Setup as a slide-in drawer; Dialog/Answers carry badges. */}
          <div className="assistant-tabs" role="tablist">
            <button
              className={"tab-btn tab-icon" + (settingsOpen ? " on" : "")}
              title="Setup"
              aria-expanded={settingsOpen}
              onClick={() => setSettingsOpen((o) => !o)}
            >
              <SlidersHorizontal size={16} />
            </button>
            <button
              className={"tab-btn" + (pane === "dialog" ? " on" : "")}
              role="tab"
              aria-selected={pane === "dialog"}
              onClick={() => setPane("dialog")}
            >
              <MessageSquare size={15} /> Dialog
              {dialogUnread > 0 && <span className="tab-badge">{badge(dialogUnread)}</span>}
            </button>
            <button
              className={"tab-btn" + (pane === "answers" ? " on" : "")}
              role="tab"
              aria-selected={pane === "answers"}
              onClick={() => setPane("answers")}
            >
              <Sparkles size={15} /> Answers
              {answersUnread > 0 && <span className="tab-badge">{badge(answersUnread)}</span>}
            </button>
          </div>

          <div className={"assistant-body pane-" + pane + (settingsOpen ? " settings-open" : "")}>
          <section className="assistant-col col-settings">
          {/* Drawer header - shown only in the narrow layout where Setup is a drawer. */}
          <div className="drawer-head">
            <span>Setup</span>
            <button className="icon-btn sm" title="Close" onClick={() => setSettingsOpen(false)}>
              <X size={16} />
            </button>
          </div>
          {needsDeepgramKey ? (
            <div className="add-source onboard">
              <h3>
                <Ear size={16} /> Deepgram API key
              </h3>
              <p className="muted sm">
                The Cloud engine sends captured audio to Deepgram to transcribe it, so audio leaves your
                machine. Get a key at console.deepgram.com - or switch the engine to Local above to keep
                everything on-device.
              </p>
              <div className="add-row">
                <input
                  type="password"
                  placeholder="Deepgram API key"
                  value={sttKeyInput}
                  onChange={(e) => setSttKeyInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void saveSttKey();
                  }}
                />
                <button className="btn-open" onClick={saveSttKey} disabled={savingStt || !sttKeyInput.trim()}>
                  {savingStt ? "Saving..." : "Save key"}
                </button>
              </div>
            </div>
          ) : (
            <div className="listen-bar">
              <button className={"btn-listen primary" + (listening ? " on" : "")} onClick={toggleListen}>
                {listening ? <Square size={15} /> : <Ear size={15} />}
                {listening ? "Stop listening" : "Listen to call"}
              </button>
              <label className="auto-toggle muted sm">
                <input
                  type="checkbox"
                  checked={autoAnswer}
                  onChange={(e) => setAutoAnswer(e.target.checked)}
                />
                Auto-answer questions
              </label>
            </div>
          )}

          {listenError && <div className="assistant-error">{listenError}</div>}

          {listening && (
            <div className={"capture-diag" + (level.peak < 0.005 ? " warn" : " ok")}>
              <div className="capture-diag-head">
                <span className="muted sm">Capture diagnostics</span>
                <span className="muted sm">
                  peak {level.peak.toFixed(3)} · {Math.round(level.buffered / 1000)}k buffered
                </span>
              </div>
              <div className="level-track">
                <div
                  className={"level-fill" + (level.peak >= 0.005 ? " live" : "")}
                  style={{ width: `${Math.min(100, Math.round(level.peak * 140))}%` }}
                />
              </div>
              {captureInfo.length === 0 ? (
                <div className="muted sm">Opening capture device…</div>
              ) : (
                captureInfo.map((d) => (
                  <div key={d.role} className="muted sm">
                    {d.role}: <strong>{d.name}</strong> · {Math.round(d.rate / 1000)}kHz · {d.channels}ch
                  </div>
                ))
              )}
              <div className="diag-hint muted sm">
                {level.peak < 0.005
                  ? "No sound reaching this device. If capturing System audio, set the Output device above to the one your call plays through."
                  : "Audio is being captured. If nothing transcribes, it's the transcription engine, not capture."}
              </div>
            </div>
          )}

          <div className="listen-bar screen-bar">
            <span className="muted sm">
              <Monitor size={14} /> Screen:
            </span>
            <select
              value={screenMonitor}
              onChange={(e) => setScreenMonitor(Number(e.target.value))}
              disabled={watchingScreen}
              title="Which monitor to read questions from"
            >
              {monitors.length === 0 ? (
                <option value={0}>Primary display</option>
              ) : (
                monitors.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))
              )}
            </select>
            <button
              className={"btn-listen" + (watchingScreen ? " on" : "")}
              onClick={() => setWatchingScreen((v) => !v)}
            >
              {watchingScreen ? <Square size={15} /> : <ScanSearch size={15} />}
              {watchingScreen ? "Stop watching" : "Watch screen"}
            </button>
            <button className="btn-ghost" onClick={() => void runScreen(true)} disabled={watchingScreen}>
              Answer screen now
            </button>
            <button className="btn-ghost" onClick={() => void startSnip()} disabled={watchingScreen} title="Snip a region of the screen and explain it">
              <Crop size={14} /> Snip
            </button>
            <button
              className="btn-ghost"
              onClick={() => void readPage()}
              title="Copy the page in your browser (Ctrl+A, Ctrl+C) then click to attach it as context - including off-screen content. On a coding page, also click into the editor, copy the code, and click again to add it. Each click appends; use Clear to reset."
            >
              <ClipboardPaste size={14} /> Read page
            </button>
          </div>
          {pageMsg && <div className="muted sm">{pageMsg}</div>}
          <div className="muted sm">
            Can't copy (proctored / locked page)? Press <span className="kbd">Alt+Shift+R</span> while the page
            is focused to read it via accessibility (Windows).
          </div>
          {screenError && <div className="assistant-error">{screenError}</div>}

          <details className="acc" open>
            <summary>Setup</summary>
            <div className={"acc-body" + (listening ? " is-listening" : "")}>

            <div className="setup-row">
              <span className="setup-label">Answer</span>
              <div className="seg" role="group" aria-label="Answer engine">
                <button
                  className={"seg-btn" + (answerEngine === "claude" ? " on" : "")}
                  aria-pressed={answerEngine === "claude"}
                  title="Claude (cloud) - sharpest answers; uses your Anthropic API key"
                  onClick={() => void changeAnswerEngine("claude")}
                >
                  Claude
                </button>
                <button
                  className={"seg-btn" + (answerEngine === "ollama" ? " on" : "")}
                  aria-pressed={answerEngine === "ollama"}
                  title="Local (Ollama) - free + private, runs on your machine; requires Ollama"
                  onClick={() => void changeAnswerEngine("ollama")}
                >
                  Local
                </button>
              </div>
            </div>

            <div className="setup-row">
              <span className="setup-label">Answer style</span>
              <select
                value={answerStyle}
                title="How answers are formatted. Auto lets the model pick per question."
                onChange={(e) => void changeStyle(e.target.value)}
              >
                {ANSWER_STYLES.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.label} - {s.note}
                  </option>
                ))}
              </select>
            </div>

            {answerEngine === "ollama" && (
              <div className="setup-row">
                <span className="setup-label">Ollama model</span>
                {ollamaModels.length > 0 ? (
                  <select
                    value={ollamaModel}
                    onChange={(e) => void changeOllamaModel(e.target.value)}
                    title="Local model used to answer (run `ollama pull <model>` to add more)"
                  >
                    <option value="">Select a model…</option>
                    {ollamaModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                ) : (
                  <span className="muted sm">
                    No models found - start Ollama, then <code>ollama pull llama3.1</code>.
                  </span>
                )}
              </div>
            )}

            <div className="setup-divider" />

            <div className="setup-row">
              <span className="setup-label">Transcription</span>
              <div className="seg" role="group" aria-label="Transcription engine">
                <button
                  className={"seg-btn" + (sttEngine === "deepgram" ? " on" : "")}
                  aria-pressed={sttEngine === "deepgram"}
                  disabled={listening}
                  title="Cloud (Deepgram) - captured audio is sent to Deepgram to transcribe"
                  onClick={() => void changeEngine("deepgram")}
                >
                  Cloud
                </button>
                <button
                  className={"seg-btn" + (sttEngine === "local" ? " on" : "")}
                  aria-pressed={sttEngine === "local"}
                  disabled={listening}
                  title="Local (Whisper) - runs on-device, audio never leaves your machine"
                  onClick={() => void changeEngine("local")}
                >
                  Local
                </button>
              </div>
            </div>

            {sttEngine === "deepgram" && (
              <>
                <div className="setup-row">
                  <span className="setup-label">Language</span>
                  <select
                    value={sttLanguage}
                    disabled={listening}
                    title="Spoken language for Deepgram. Streaming can't auto-detect, so pick the language of the call (or Multilingual)."
                    onChange={(e) => void changeLanguage(e.target.value)}
                  >
                    {STT_LANGUAGES.map((l) => (
                      <option key={l.code} value={l.code}>
                        {l.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="setup-row">
                  <label
                    className="switch"
                    title="Separate the other side into Speaker 1/2/... Leave off for 1-on-1 calls - it tends to mislabel one person as several."
                  >
                    <input
                      type="checkbox"
                      checked={sttDiarize}
                      disabled={listening}
                      onChange={(e) => void changeDiarize(e.target.checked)}
                    />
                    Separate speakers
                  </label>
                </div>
              </>
            )}

            <div className="setup-row">
              <span className="setup-label">Capture</span>
              <div className="seg" role="group" aria-label="Capture source">
                <button
                  className={"seg-btn" + (captureSource === "system" ? " on" : "")}
                  aria-pressed={captureSource === "system"}
                  disabled={listening}
                  title="System audio - the other side of the call/video (via loopback)"
                  onClick={() => void changeCaptureSource("system")}
                >
                  System
                </button>
                <button
                  className={"seg-btn" + (captureSource === "mic" ? " on" : "")}
                  aria-pressed={captureSource === "mic"}
                  disabled={listening}
                  title="Microphone - your voice"
                  onClick={() => void changeCaptureSource("mic")}
                >
                  Mic
                </button>
                <button
                  className={"seg-btn" + (captureSource === "both" ? " on" : "")}
                  aria-pressed={captureSource === "both"}
                  disabled={listening}
                  title="System + Mic - both sides of the call"
                  onClick={() => void changeCaptureSource("both")}
                >
                  Both
                </button>
              </div>
            </div>

            <div className="setup-divider" />

            {captureSource !== "mic" && (
              <div className="setup-row">
                <span className="setup-label">Output device</span>
                <select
                  value={outputDevice}
                  onChange={(e) => void changeOutputDevice(e.target.value)}
                  disabled={listening}
                  title="Which speaker/output to capture (its sound is recorded via loopback)"
                >
                  <option value="">Default output</option>
                  {devices.outputs.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </div>
            )}

            {captureSource !== "system" && (
              <div className="setup-row">
                <span className="setup-label">Input device</span>
                <select
                  value={inputDevice}
                  onChange={(e) => void changeInputDevice(e.target.value)}
                  disabled={listening}
                  title="Which microphone to capture"
                >
                  <option value="">Default microphone</option>
                  {devices.inputs.map((d) => (
                    <option key={d} value={d}>
                      {d}
                    </option>
                  ))}
                </select>
              </div>
            )}
            </div>
          </details>

          <details className="acc">
            <summary>Overlay</summary>
            <div className="acc-body">
            <div className="overlay-row">
              <span className="setup-label">Opacity</span>
              <input
                type="range"
                min={0.3}
                max={1}
                step={0.01}
                value={opacity}
                onChange={(e) => setOpacity(Number(e.target.value))}
                title="Make the window translucent so it sits over a call (Windows)"
                style={
                  {
                    "--range-fill": `linear-gradient(to right, var(--accent) ${opacityPct}%, var(--border) ${opacityPct}%)`,
                  } as CSSProperties
                }
              />
              <span className="muted sm">{Math.round(opacity * 100)}%</span>
            </div>
            <label className="auto-toggle muted sm">
              <input type="checkbox" checked={onTop} onChange={(e) => setOnTop(e.target.checked)} />
              Always on top
            </label>
            <div className="muted sm">Dims the whole window (this UI too); min 30%.</div>
            </div>
          </details>

          <details className="acc">
            <summary>Interview context</summary>
            <div className="acc-body">
            <p className="muted sm">Fed to the assistant so answers fit you and the role.</p>
            <div className="setup-field">
              <span className="setup-label">Target role / focus</span>
              <input
                value={contextRole}
                onChange={(e) => setContextRole(e.target.value)}
                placeholder="e.g. Senior Backend Engineer"
              />
            </div>
            <div className="setup-field">
              <span className="setup-label">Resume / background</span>
              <textarea
                rows={4}
                value={contextResume}
                onChange={(e) => setContextResume(e.target.value)}
                placeholder="Paste your resume or a short summary…"
              />
            </div>
            <div className="setup-field">
              <span className="setup-label">Notes / materials</span>
              <textarea
                rows={3}
                value={contextNotes}
                onChange={(e) => setContextNotes(e.target.value)}
                placeholder="Job description, talking points, anything to tailor answers…"
              />
            </div>
            <button className="btn-open" onClick={() => void saveContext()}>
              {contextSaved ? "Saved ✓" : "Save context"}
            </button>
            </div>
          </details>

          <details className="acc">
            <summary>Your stories</summary>
            <div className="acc-body">
              <StoriesPanel />
            </div>
          </details>

          <details className="acc">
            <summary>Past sessions</summary>
            <div className="acc-body">
              <SessionsPanel reload={sessionsReload} />
            </div>
          </details>

          <details className="acc">
            <summary>Company information</summary>
            <div className="acc-body">
            <p className="muted sm">
              Add the company you're interviewing at - answers become company-aware, and you can generate a
              briefing + questions to ask.
            </p>
            <div className="setup-field">
              <span className="setup-label">Company (name, site, or notes)</span>
              <textarea
                rows={3}
                value={contextCompany}
                onChange={(e) => setContextCompany(e.target.value)}
                placeholder="e.g. Stripe - payments infrastructure. Anything you know: products, team, JD…"
              />
            </div>
            <button
              className="btn-sync"
              onClick={() => void generateCompanyBrief()}
              disabled={!contextCompany.trim()}
            >
              <Send size={15} /> Generate briefing &amp; questions
            </button>
            <div className="muted sm">
              Uses the model's own knowledge + what you enter (no live web lookup), so verify specifics.
            </div>
            </div>
          </details>

          <details className="acc">
            <summary>Application kit</summary>
            <div className="acc-body">
              <ApplicationKit transcript={transcriptText(segments)} />
            </div>
          </details>

          <details
            className="acc"
            onToggle={(e) => {
              if ((e.target as HTMLDetailsElement).open) void refreshUsage();
            }}
          >
            <summary>Usage &amp; cost</summary>
            <div className="acc-body">
              {/* Claude: token-based spend estimate, optional credit -> remaining. */}
              <div className="usage-row">
                <span className="usage-name">Claude</span>
                <span className="usage-val">
                  {usage && usage.claudeCredit > 0
                    ? `$${Math.max(0, usage.claudeCredit - claudeCost).toFixed(2)} left`
                    : `$${claudeCost.toFixed(2)} used`}
                </span>
              </div>
              <div className="muted sm">
                {usage?.since
                  ? `~$${claudeCost.toFixed(2)} est. (${(claudeTokens / 1000).toFixed(0)}k tokens) since ${new Date(
                      usage.since * 1000,
                    ).toLocaleDateString()}`
                  : "No Claude usage tracked yet."}
              </div>
              {(usage?.claudeItems ?? []).map((it) => (
                <div key={it.model} className="muted sm usage-line">
                  {modelLabel(it.model)}: {(it.input / 1000).toFixed(0)}k in / {(it.output / 1000).toFixed(0)}k out
                </div>
              ))}
              <label className="usage-credit">
                <span className="setup-label">Credit added ($)</span>
                <input
                  type="number"
                  min={0}
                  step="0.01"
                  placeholder="e.g. 20"
                  value={claudeCreditInput}
                  onChange={(e) => setClaudeCreditInput(e.target.value)}
                  onBlur={() => void saveCredit("claude", claudeCreditInput)}
                />
              </label>
              <div className="usage-actions">
                <button className="link-btn" onClick={() => void resetUsage()}>
                  Reset spend
                </button>
                <button
                  className="link-btn"
                  onClick={() => void openUrl("https://console.anthropic.com/settings/usage")}
                >
                  Open Anthropic console
                </button>
              </div>
              <div className="muted sm">
                Spend is estimated from token counts and list prices, then subtracted from the credit you
                enter - the exact balance lives in the Anthropic console.
              </div>

              <hr className="usage-sep" />

              {/* Deepgram: live balance when the key allows it; otherwise a minute-based estimate. */}
              <div className="usage-row">
                <span className="usage-name">Deepgram</span>
                <span className="usage-val">
                  {dgLoading
                    ? "…"
                    : dgBalance
                      ? `$${dgBalance.amount.toFixed(2)} left`
                      : usage && usage.deepgramCredit > 0
                        ? `$${Math.max(0, usage.deepgramCredit - dgCost).toFixed(2)} left`
                        : `$${dgCost.toFixed(2)} used`}
                </span>
              </div>
              {dgBalance ? (
                <div className="muted sm">Live balance{dgBalance.project ? ` · ${dgBalance.project}` : ""}</div>
              ) : (
                <div className="muted sm">
                  {`~$${dgCost.toFixed(2)} est. (${dgMinutes.toFixed(1)} min streamed)`}
                  {dgError ? " · live balance unavailable for this key" : ""}
                </div>
              )}
              {!dgBalance && (
                <label className="usage-credit">
                  <span className="setup-label">Credit added ($)</span>
                  <input
                    type="number"
                    min={0}
                    step="0.01"
                    placeholder="e.g. 25"
                    value={dgCreditInput}
                    onChange={(e) => setDgCreditInput(e.target.value)}
                    onBlur={() => void saveCredit("deepgram", dgCreditInput)}
                  />
                </label>
              )}
              <div className="usage-actions">
                <button className="link-btn" onClick={() => void refreshUsage()} disabled={dgLoading}>
                  Refresh
                </button>
                <button className="link-btn" onClick={() => void openUrl("https://console.deepgram.com/")}>
                  Open Deepgram console
                </button>
              </div>
              {dgError && (
                <div className="muted sm">
                  Live balance needs an Owner/Admin Deepgram key; a transcription key can't read it. Enter your
                  credit above for an estimate, or check the console.
                </div>
              )}

              <hr className="usage-sep" />
              <div className="muted sm">Local (Ollama / Whisper): free - runs on your machine.</div>
            </div>
          </details>
          </section>

          <section className="assistant-col col-dialog">
            <div className="dialog-head">
              <span className="muted sm">{listening ? listenStatus || "Listening..." : "Live dialog"}</span>
              <span className="transcript-actions">
                {(segments.length > 0 || qa.length > 0) && (
                  <button className="link-btn" onClick={() => void saveSession()}>
                    {sessionSaved ? "Saved ✓" : "Save session"}
                  </button>
                )}
                {segments.length > 0 && (
                  <button className="link-btn" onClick={() => setContext(transcriptText(segments))}>
                    Use as context
                  </button>
                )}
                {segments.length > 0 && (
                  <button className="link-btn" onClick={clearTranscript}>
                    Clear
                  </button>
                )}
              </span>
            </div>
            {listening || segments.length > 0 ? (
              <div className="dialog-list" aria-live="polite">
                {segments.map((s) => (
                  <div key={s.id} className={"seg-msg seg-" + s.role}>
                    {s.role === "them" && (
                      <span className={"seg-chip " + chipClass(s.speaker)}>{s.speaker}</span>
                    )}
                    <div className="seg-bubble">{s.text}</div>
                  </div>
                ))}
                {(["them", "you"] as const).map((role) => {
                  const t = `${live[role].fin} ${live[role].int}`.trim();
                  return t ? (
                    <div key={"live-" + role} className={"seg-msg seg-" + role + " seg-interim"}>
                      {role === "them" && <span className="seg-chip seg-chip--live">Speaking…</span>}
                      <div className="seg-bubble">
                        {t}
                        <span className="seg-caret" />
                      </div>
                    </div>
                  ) : null;
                })}
              </div>
            ) : (
              <div className="dialog-empty">
                <Ear size={28} />
                <span>
                  Hit <strong>Listen to call</strong> - the conversation appears here, you on the right,
                  the other side on the left.
                </span>
              </div>
            )}

          <div className="assistant-ask">
            <textarea
              placeholder="Ask directly, e.g. 'Solve 3x + 7 = 22' - or just hit Listen and let it auto-answer"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) askManual();
              }}
              rows={2}
            />
            <button className="btn-sync" onClick={askManual} disabled={!question.trim()}>
              <Send size={16} /> Ask
            </button>
          </div>
          {context && (
            <div className="muted sm context-note">
              Context attached ({context.length} chars).{" "}
              <button className="link-btn" onClick={() => setContext("")}>
                clear
              </button>
            </div>
          )}

          </section>

          <aside className="assistant-col col-code">
            <section className="col-section live-answers">
              <div className="pane-title">Live answers</div>
              <div className="col-scroll">
                {qa.length === 0 ? (
                  <div className="pane-empty">
                    <strong>No answers yet.</strong>
                    <span className="muted sm">Ask, auto-answer, or snip - answers stream in here.</span>
                  </div>
                ) : (
                  qa.map((e) => <QaCard key={e.id} e={e} />)
                )}
              </div>
            </section>
            <section className="col-section code-math">
              <div className="pane-title">Code &amp; Math</div>
              <div className="col-scroll">
                {cmGroups.length === 0 ? (
                  <div className="pane-empty">
                    <span className="muted sm">Code blocks &amp; equations pulled from answers collect here.</span>
                  </div>
                ) : (
                  cmGroups.map((g, gi) => (
                    <div className={"cm-group" + (gi === 0 ? " latest" : "")} key={g.id}>
                      <div className="cm-group-q">{g.q}</div>
                      {g.items.map((s) => (
                        <div className="cm-card" key={s.key}>
                          <Answer text={s.md} />
                        </div>
                      ))}
                    </div>
                  ))
                )}
              </div>
            </section>
          </aside>
          {narrow && settingsOpen && (
            <div className="settings-backdrop" onClick={() => setSettingsOpen(false)} />
          )}
          </div>
        </>
      )}

      {snipSrc && (
        <SnipOverlay src={snipSrc} onCancel={() => setSnipSrc(null)} onSelect={onSnipSelect} />
      )}
    </div>
  );
}
