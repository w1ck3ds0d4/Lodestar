import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type CSSProperties,
} from "react";
import { Channel } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Bot,
  Briefcase,
  Building2,
  ClipboardPaste,
  Crop,
  Ear,
  FileText,
  Gauge,
  History,
  ImagePlus,
  KeyRound,
  LayoutDashboard,
  MessageSquare,
  Monitor,
  ScanSearch,
  ScrollText,
  Send,
  ShieldCheck,
  SlidersHorizontal,
  Smartphone,
  Sparkles,
  Square,
  Upload,
  X,
} from "lucide-react";
import * as api from "../api";
import {
  ANSWER_STYLES,
  ASSISTANT_MODELS,
  DEEPGRAM_PER_MIN,
  STT_LANGUAGES,
  estimateCost,
  type AudioDevices,
  type CaptureCommand,
  type CaptureSegment,
  type CaptureState,
  type DeepgramBalance,
  type LogEntry,
  type MonitorInfo,
  type PhoneInfo,
  type PhoneNetStatus,
  type Profile,
  type UsageReport,
} from "../types";
import { isBackchannel, isNoneAnswer, looksNone } from "../lib/answer";
import { extractResumeText } from "../lib/extractText";
import { ApplicationKit } from "./ApplicationKit";
import { Answer } from "./MarkdownAnswer";
import { SessionsPanel } from "./SessionsPanel";
import { StoriesPanel } from "./StoriesPanel";

interface QA {
  id: number;
  q: string;
  a: string;
  pending: boolean;
  error?: string;
  // A thumbnail (base64 PNG) of the screen/image the answer is about, shown as the question
  // for screen and image asks (where `q` is only a generic label like "From your screen").
  image?: string;
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
  // Capture time (epoch ms) so the dialog can show timestamps + response latency.
  ts: number;
}

// HH:MM:SS.mmm for a dialog line, so the user can see how long each turn took.
function fmtSegTime(ts: number): string {
  const d = new Date(ts);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

// HH:MM:SS for an activity-log entry (no milliseconds - these are user actions, not turns).
function fmtLogClock(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

// Downscale an image (given an <img> src) to a base64 PNG whose longest side is <= maxSide,
// resolving to base64 with no "data:" prefix. `onSettled` runs once the source has loaded/failed
// (used to revoke object URLs).
function srcToPngBase64(src: string, maxSide: number, onSettled?: () => void): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      onSettled?.();
      const scale = Math.min(1, maxSide / Math.max(img.naturalWidth, img.naturalHeight));
      const w = Math.max(1, Math.round(img.naturalWidth * scale));
      const h = Math.max(1, Math.round(img.naturalHeight * scale));
      const canvas = document.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d");
      if (!ctx) return reject(new Error("canvas unavailable"));
      ctx.drawImage(img, 0, 0, w, h);
      resolve(canvas.toDataURL("image/png").split(",")[1] || "");
    };
    img.onerror = () => {
      onSettled?.();
      reject(new Error("could not read the image"));
    };
    img.src = src;
  });
}

// A pasted/dropped image file -> base64 PNG (longest side <= maxSide, Claude's efficient max).
// Normalizing to PNG means the backend's media_type is always correct.
function imageToPngBase64(file: Blob, maxSide = 1568): Promise<string> {
  const url = URL.createObjectURL(file);
  return srcToPngBase64(url, maxSide, () => URL.revokeObjectURL(url));
}

// A base64 PNG -> a small base64 PNG thumbnail, shown as the "question" for screen/image asks.
function pngThumb(base64: string, maxSide = 400): Promise<string> {
  return srcToPngBase64("data:image/png;base64," + base64, maxSide);
}

// Stable colour bucket for a speaker name (so each speaker keeps one chip hue).
function chipClass(speaker: string): string {
  let h = 0;
  for (const c of speaker) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return "seg-chip--c" + (h % 5);
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
      if (body.trim())
        items.push({ key: `${e.id}-c${i++}`, md: "```" + (m[1] || "") + "\n" + body + "\n```" });
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

const QaCard = memo(function QaCard({ e }: { e: QA }) {
  return (
    <div className="qa">
      {e.image && (
        <img
          className="qa-image"
          src={"data:image/png;base64," + e.image}
          alt="the screen or image this answers"
        />
      )}
      <div className="qa-q" title={e.q}>
        {e.q}
      </div>
      {e.error ? (
        <div className="assistant-error">{e.error}</div>
      ) : e.pending && !e.a ? (
        // A sayable bridge the user can read aloud to kill dead air while the answer streams.
        <div className="stall-bridge">Good question - let me give you a structured answer.</div>
      ) : (
        <Answer text={e.a} live />
      )}
    </div>
  );
});

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
  // `spk` retains the diarized speaker label ("Speaker N") from the worded spans of the
  // open utterance, so a trailing empty speech-final endpoint (which carries only the
  // generic role label) can't overwrite it when the bubble commits.
  const emptyLive = () => ({
    you: { fin: "", int: "", spk: "" },
    them: { fin: "", int: "", spk: "" },
  });
  const liveRef = useRef(emptyLive());
  const [live, setLive] = useState(emptyLive());
  const [listenError, setListenError] = useState("");
  const [autoAnswer, setAutoAnswer] = useState(true);
  const [captureInfo, setCaptureInfo] = useState<CaptureDevice[]>([]);
  const [level, setLevel] = useState<{ mic: number; sys: number; buffered: number }>({
    mic: 0,
    sys: 0,
    buffered: 0,
  });

  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [screenMonitor, setScreenMonitor] = useState(0);
  // Activity log: a running record of what each action/shortcut did, so it's clear that e.g.
  // "watch" (screenshots + answers) and "read page" (text -> context) are different things.
  const [logEntries, setLogEntries] = useState<LogEntry[]>([]);
  const logIdRef = useRef(0);
  const pushLog = useCallback((kind: string, msg: string) => {
    setLogEntries((prev) => [{ id: ++logIdRef.current, ts: Date.now(), kind, msg }, ...prev].slice(0, 60));
  }, []);
  const [watchingScreen, setWatchingScreen] = useState(false);
  const [screenError, setScreenError] = useState("");
  const [screenNote, setScreenNote] = useState("");
  const [snipSrc, setSnipSrc] = useState<string | null>(null);
  const [pageMsg, setPageMsg] = useState("");
  const [sessionsReload, setSessionsReload] = useState(0);
  const [sessionSaved, setSessionSaved] = useState(false);
  const screenBusy = useRef(false);

  const [opacity, setOpacity] = useState(1);
  const [onTop, setOnTop] = useState(false);
  // Hide the app's taskbar button (discreet mode). Persisted so it survives a restart;
  // Alt+Shift+H still shows/hides the window, so it can always be brought back.
  const [skipTaskbar, setSkipTaskbar] = useState(
    () => typeof localStorage !== "undefined" && localStorage.getItem("jr.skipTaskbar") === "1",
  );

  const [contextRole, setContextRole] = useState("");
  const [contextCompany, setContextCompany] = useState("");
  const [contextResume, setContextResume] = useState("");
  const [contextNotes, setContextNotes] = useState("");
  const [contextSaved, setContextSaved] = useState(false);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [activeProfileId, setActiveProfileId] = useState<number | null>(null);
  const [resumeMsg, setResumeMsg] = useState("");
  const resumeFileRef = useRef<HTMLInputElement>(null);

  const [usage, setUsage] = useState<UsageReport | null>(null);
  const [claudeCreditInput, setClaudeCreditInput] = useState("");
  const [dgCreditInput, setDgCreditInput] = useState("");
  const [claudeSpentInput, setClaudeSpentInput] = useState("");
  const [dgSpentInput, setDgSpentInput] = useState("");
  const [dgBalance, setDgBalance] = useState<DeepgramBalance | null>(null);
  const [dgError, setDgError] = useState("");
  const [dgLoading, setDgLoading] = useState(false);

  const [question, setQuestion] = useState("");
  // An image attached to the Ask box (base64 PNG) - pasted, dropped, or picked.
  const [askImage, setAskImage] = useState<string | null>(null);
  const imgInputRef = useRef<HTMLInputElement>(null);
  const [context, setContext] = useState("");
  const [qa, setQa] = useState<QA[]>([]);

  // Phone companion: LAN server connection details (null while stopped).
  const [phone, setPhone] = useState<PhoneInfo | null>(null);
  const [phoneErr, setPhoneErr] = useState("");
  const [phoneBusy, setPhoneBusy] = useState(false);
  const [phoneNet, setPhoneNet] = useState<PhoneNetStatus | null>(null);
  const [fixingFw, setFixingFw] = useState(false);

  // Capture mode: floating overlay panels driven by this window.
  const [captureOn, setCaptureOn] = useState(false);
  const [captureErr, setCaptureErr] = useState("");
  // Guards start/stop so the desktop button, a capture-panel command, and the phone
  // can't race and desync `captureOn` from the actual panel windows.
  const captureBusy = useRef(false);
  // Whether the capture panels are hidden from screen capture (default true). Turn off
  // to screenshot the panels / show them in a share.
  const [captureProtect, setCaptureProtect] = useState(true);

  const idRef = useRef(0);
  const segIdRef = useRef(0);
  const answeredIdx = useRef(0);
  // The dialog scroll container, kept pinned to the newest line as the conversation
  // grows (a normal top-to-bottom chat). `stickToBottom` turns off while the user
  // scrolls up to read back, so we never yank them away from what they're reading.
  const dialogListRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  // Synchronous mirror of the dialog so the screen watcher can read the current
  // conversation as context without re-creating runScreen (which would reset the
  // watch interval on every new line).
  const segmentsRef = useRef<DialogSegment[]>([]);
  // Mirror of the Q&A feed, so auto-save can read the final state after listening
  // stops (the stop flush commits asynchronously, so the closures would be stale).
  const qaRef = useRef<QA[]>([]);
  // Auto-save bookkeeping: whether the current listening session was already saved
  // (manually or automatically), and whether we were listening on the last render.
  const sessionSavedRef = useRef(false);
  const wasListeningRef = useRef(false);
  // Synchronous mirror of `context` so the hotkey event listener and Read page can
  // dedupe/append without a stale closure.
  const contextRef = useRef("");
  useEffect(() => {
    contextRef.current = context;
  }, [context]);
  useEffect(() => {
    segmentsRef.current = segments;
  }, [segments]);
  useEffect(() => {
    qaRef.current = qa;
  }, [qa]);
  // When ON, reading a page (Read full page / Alt+Shift+R / clipboard) also answers it - works
  // whether or not you're in a live call (e.g. a live coding round). Turn OFF to only stack the
  // page as context (e.g. read several parts, then Ask once). Persisted; a ref so the
  // once-registered read handlers see the latest value.
  const [solveOnRead, setSolveOnRead] = useState(() => localStorage.getItem("jr.solveOnRead") !== "0");
  const solveOnReadRef = useRef(solveOnRead);
  useEffect(() => {
    solveOnReadRef.current = solveOnRead;
    localStorage.setItem("jr.solveOnRead", solveOnRead ? "1" : "0");
  }, [solveOnRead]);

  // Follow the conversation: scroll the dialog to the newest line whenever a segment
  // commits or the live tail grows - unless the user has scrolled up to read back.
  useEffect(() => {
    const el = dialogListRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [segments, live, listening]);

  // Append a captured chunk (clipboard or focused-window text) to the assistant's
  // context, de-duplicating repeats and capping the total so it stays affordable.
  const addToContext = useCallback(
    (raw: string, label: string) => {
      const text = raw.trim();
      if (!text) {
        setPageMsg(`${label}: nothing captured.`);
        pushLog("read", `${label}: nothing captured`);
        window.setTimeout(() => setPageMsg(""), 3500);
        return;
      }
      const prev = contextRef.current;
      if (prev.includes(text)) {
        setPageMsg(`${label}: already attached. Copy another part (e.g. the code editor) to add more.`);
        pushLog("read", `${label}: already attached (deduped, ${text.length.toLocaleString()} chars)`);
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
      pushLog(
        "read",
        `${label}: +${text.length.toLocaleString()} chars to context (total ${next.length.toLocaleString()})`,
      );
      window.setTimeout(() => setPageMsg(""), 3500);
    },
    [pushLog],
  );

  // Answer a read-in page directly: the page text becomes the question, streamed into a card
  // labelled "From the page" (so the whole page isn't shown as the header). Capped so a huge
  // page can't blow the request up.
  const answerPage = useCallback((pageText: string) => {
    const text = pageText.trim().slice(0, 40000);
    if (text.length < 2) return;
    const id = ++idRef.current;
    setQa((prev) => [{ id, q: "From the page", a: "", pending: true }, ...prev]);
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
    void api
      .askAssistant(text, null, channel)
      .catch((err) =>
        setQa((prev) => prev.map((x) => (x.id === id ? { ...x, error: String(err), pending: false } : x))),
      );
  }, []);

  // Attach a read-in page as context AND, when "Solve on read" is on, answer it - so an explicit
  // page read (Read full page / Alt+Shift+R / clipboard) also solves the exercise, in a call or
  // not. With the toggle off it stays context-only (stack several parts, then Ask once).
  const readPageAndAnswer = useCallback(
    (text: string, label: string) => {
      addToContext(text, label);
      if (solveOnReadRef.current) answerPage(text);
    },
    [addToContext, answerPage],
  );

  // Overlay window controls. Applied after mount (hwnd() errors before the window
  // exists); opacity is debounced so dragging the slider doesn't spam invoke. The same
  // value also dims the capture overlay panels (a no-op when none are open), so the opacity
  // slider and the phone's opacity buttons fade the floating panels along with the window.
  useEffect(() => {
    const t = setTimeout(() => {
      void api.setWindowOpacity(opacity).catch(() => {});
      void api.setCaptureOpacity(opacity).catch(() => {});
    }, 60);
    return () => clearTimeout(t);
  }, [opacity]);
  useEffect(() => {
    void api.setWindowAlwaysOnTop(onTop).catch(() => {});
  }, [onTop]);
  // Apply the taskbar-hide preference on mount and whenever it changes, and remember it.
  useEffect(() => {
    try {
      localStorage.setItem("jr.skipTaskbar", skipTaskbar ? "1" : "0");
    } catch {
      /* ignore storage errors */
    }
    void api.setWindowSkipTaskbar(skipTaskbar).catch(() => {});
  }, [skipTaskbar]);

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
      setCaptureProtect(s.captureProtect);
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

  // Import a resume file (PDF/DOCX/TXT/MD) into the resume field; the user reviews
  // and then hits Save context.
  const onResumeFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = ""; // let the same file be re-picked later
    if (!file) return;
    setResumeMsg("Reading...");
    try {
      const text = await extractResumeText(file);
      if (!text.trim()) {
        setResumeMsg("No selectable text found in that file (a scanned PDF has none).");
        return;
      }
      setContextResume(text);
      setResumeMsg(
        `Imported ${text.length.toLocaleString()} chars from ${file.name} - review and Save context.`,
      );
    } catch (err) {
      setResumeMsg(String(err));
    }
  };
  const refreshOllamaModels = useCallback(() => {
    void api
      .listOllamaModels()
      .then(setOllamaModels)
      .catch(() => setOllamaModels([]));
  }, []);
  const loadProfiles = useCallback(() => {
    void Promise.all([api.listProfiles(), api.activeProfile()])
      .then(([list, active]) => {
        setProfiles(list);
        setActiveProfileId(active);
      })
      .catch(() => {});
  }, []);
  // Switch the active profile from the assistant: persist it, then reload the context
  // fields (read from the active profile via the mirror) and the profile list.
  const switchProfile = useCallback(
    async (id: number) => {
      setActiveProfileId(id);
      try {
        await api.setActiveProfile(id);
        await loadSettings();
        loadProfiles();
      } catch {
        /* ignore */
      }
    },
    [loadSettings, loadProfiles],
  );
  useEffect(() => {
    void loadSettings();
    void api
      .listAudioDevices()
      .then(setDevices)
      .catch(() => {});
    void api
      .listMonitors()
      .then(setMonitors)
      .catch(() => {});
    refreshOllamaModels();
    loadProfiles();
  }, [loadSettings, refreshOllamaModels, loadProfiles]);

  // Capture the chosen screen and append any answer to the Q&A feed. `force`
  // answers even an unchanged screen (manual button); auto-watch passes false so
  // a static screen costs nothing. A busy guard prevents overlapping calls.
  const runScreen = useCallback(
    async (force: boolean, monitorOverride?: number, hybrid?: boolean) => {
      if (screenBusy.current) return;
      screenBusy.current = true;
      // A phone tap can target a specific monitor; otherwise use the one picked in the UI.
      const mon = monitorOverride ?? screenMonitor;
      try {
        // Feed the recent conversation so the screen answer is context-aware.
        const convo = transcriptText(segmentsRef.current).trim();
        const ctx = convo ? convo.slice(-1500) : null;
        // Grab a thumbnail of the screen we're answering (in parallel), to show as the card's
        // "question" - otherwise a screen answer's only header is the generic "From your screen".
        const thumbP = api
          .captureScreenPng(mon)
          .then((b) => pngThumb(b))
          .catch(() => undefined);
        // Hybrid (phone Read & Answer) also reads the full page text via accessibility, so a
        // coding exercise scrolled off-screen is still captured; desktop stays screenshot-only.
        const res = hybrid
          ? await api.answerScreenHybrid(mon, force, ctx)
          : await api.answerScreen(mon, force, ctx);
        const a = res.answer.trim();
        // The backend already suppresses a perceptually-unchanged screen (res.changed),
        // so don't also dedup on answer TEXT here - two different questions can share a
        // terse answer ("True", "O(n)"), and that would wrongly swallow the second.
        const tag = hybrid ? "Read & answer" : force ? "Answer screen" : "Watch tick";
        if (res.changed && a && a.toUpperCase() !== "NONE") {
          const id = ++idRef.current;
          setQa((prev) => [{ id, q: "From your screen", a, pending: false }, ...prev]);
          setScreenNote("");
          pushLog("screen", `${tag}: answered (Display ${mon + 1})`);
          void thumbP.then((t) => {
            if (t) setQa((prev) => prev.map((e) => (e.id === id ? { ...e, image: t } : e)));
          });
        } else if (!res.changed) {
          // Skipped because the screen looks the same as the last one answered.
          pushLog("screen", `${tag}: screen unchanged, skipped (Display ${mon + 1})`);
          if (force) setScreenNote(`No question found on Display ${mon + 1}.`);
        } else if (force) {
          // A forced answer that finds nothing is otherwise a silent no-op. Say so (and which
          // display was read) so it's clear the screen was analyzed, not that the feature broke.
          setScreenNote(`No question found on Display ${mon + 1}.`);
          pushLog("screen", `${tag}: no question on screen (Display ${mon + 1})`);
        }
        setScreenError("");
      } catch (e) {
        setScreenError(String(e));
        pushLog("screen", `Screen read failed: ${String(e)}`);
      } finally {
        screenBusy.current = false;
      }
    },
    [screenMonitor, transcriptText, pushLog],
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
      readPageAndAnswer(text, "Read page");
    } catch (e) {
      setPageMsg("Couldn't read the clipboard: " + String(e));
    }
  }, [readPageAndAnswer]);

  // Read full page: pull the WHOLE focused page via accessibility (including content scrolled
  // off-screen), attach it as context (dedupe + accumulate) AND answer it when not in a live
  // call. Reads the last external window, so it works from an in-app button or the phone even
  // though those focus JobRadar itself.
  const readFullPage = useCallback(async () => {
    setPageMsg("Reading the full page...");
    try {
      const text = await api.readActivePage();
      readPageAndAnswer(text, "Read page");
    } catch (e) {
      setPageMsg("Read full page: " + String(e));
      pushLog("read", "Read full page failed: " + String(e));
      window.setTimeout(() => setPageMsg(""), 4000);
    }
  }, [readPageAndAnswer, pushLog]);

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
    pushLog("watch", "Watch screen: ON (re-reads the screen every 6s)");
    // Re-arming should answer whatever is on screen right now, even if it matches the
    // last session's screen. Force the first tick so it bypasses the backend dedup;
    // subsequent ticks poll normally.
    void runScreen(true);
    const t = setInterval(() => void runScreen(false), 6000);
    return () => {
      clearInterval(t);
      pushLog("watch", "Watch screen: OFF");
    };
  }, [watchingScreen, runScreen, pushLog]);

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

  // Ask about an attached image (a pasted/dropped screenshot) + optional text; streams like ask.
  const askImageQ = useCallback(async (q: string, image: string, ctx: string | null) => {
    const id = ++idRef.current;
    setQa((prev) => [{ id, q: q.trim() || "Image", a: "", pending: true }, ...prev]);
    // Show a thumbnail of the asked image as the card's question.
    void pngThumb(image)
      .then((t) => setQa((prev) => prev.map((e) => (e.id === id ? { ...e, image: t } : e))))
      .catch(() => {});
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
      await api.askImage(q.trim(), image, ctx && ctx.trim() ? ctx.trim() : null, channel);
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
    const add = (p: Promise<UnlistenFn>) => void p.then((u) => (cancelled ? u() : unlisteners.push(u)));
    add(
      listen<{ role: "you" | "them"; speaker: string; text: string; final?: boolean; speechFinal?: boolean }>(
        "transcript",
        (e) => {
          const { role, speaker, text, final, speechFinal } = e.payload;
          const slot = liveRef.current[role];
          const sync = () => setLive({ you: { ...liveRef.current.you }, them: { ...liveRef.current.them } });
          if (final === false) {
            // Interim refinement: replace the live tail (don't touch finalized text).
            slot.int = text;
            sync();
            return;
          }
          // Final span: append to the finalized text and clear the tail. Only on a
          // speech-final (the speaker paused) do we commit the whole utterance.
          if (text.trim()) {
            slot.fin = slot.fin ? slot.fin + " " + text.trim() : text.trim();
            slot.spk = speaker; // remember the diarized label from the span that had words
          }
          slot.int = "";
          if (speechFinal) {
            const acc = slot.fin.trim();
            // Commit under the retained speaker; fall back to the event's label only if
            // no worded span set one (so an empty endpoint never downgrades Speaker N).
            if (acc)
              setSegments((s) => [
                ...s,
                { id: ++segIdRef.current, role, speaker: slot.spk || speaker, text: acc, ts: Date.now() },
              ]);
            slot.fin = "";
            slot.spk = "";
          }
          sync();
        },
      ),
    );
    add(listen<string>("listen-status", (e) => setListenStatus(String(e.payload))));
    add(listen<string>("listen-error", (e) => setListenError(String(e.payload))));
    add(listen<CaptureDevice[]>("listen-device", (e) => setCaptureInfo(e.payload)));
    // Levels arrive per side ("mic" and/or "sys"), sometimes one at a time, so merge them.
    add(
      listen<Partial<{ mic: number; sys: number; buffered: number }>>("listen-level", (e) =>
        setLevel((prev) => ({ ...prev, ...e.payload })),
      ),
    );
    add(
      listen("listen-stopped", () => {
        // Flush any open live utterance so a sentence in progress isn't lost on stop.
        (["them", "you"] as const).forEach((role) => {
          const slot = liveRef.current[role];
          const acc = (slot.fin + " " + slot.int).trim();
          if (acc) {
            const speaker = slot.spk || (role === "them" ? "Them" : "You");
            setSegments((s) => [...s, { id: ++segIdRef.current, role, speaker, text: acc, ts: Date.now() }]);
          }
        });
        liveRef.current = emptyLive();
        setLive(emptyLive());
        setListening(false);
        setListenStatus("");
        setCaptureInfo([]);
        setLevel({ mic: 0, sys: 0, buffered: 0 });
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
    add(listen<string>("window-text", (e) => readPageAndAnswer(String(e.payload), "Read window")));
    add(
      listen<string>("window-text-error", (e) => {
        setPageMsg("Read window: " + String(e.payload));
        pushLog("read", "Read window failed: " + String(e.payload));
        window.setTimeout(() => setPageMsg(""), 4000);
      }),
    );
    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
    };
  }, [readPageAndAnswer, pushLog]);

  // Auto-answer: send each batch of new dialog segments to the model, which answers
  // it if it is a question or problem (in any language) and replies "NONE" otherwise.
  // Keyed on segment count so newly attributed lines (you/them/Speaker N) are handled.
  useEffect(() => {
    if (!autoAnswer || !listening) return;
    const pending = segments.slice(answeredIdx.current);
    if (pending.length === 0) return;
    // Over-answer safe: only fire when the unanswered tail CONTAINS a them turn. Pure
    // you-turn tails are guaranteed "NONE", so skipping them cuts wasted round-trips at
    // zero recall cost. We check "contains" (not "last segment is them") so a you reply
    // landing right after a them question still leaves that question pending to answer.
    if (!pending.some((s) => s.role === "them")) return;
    // Debounce ~400ms so a burst of speech-final segments coalesces into one call
    // instead of firing (and often retracting a NONE) on every interim finalization.
    const fire = () => {
      const newSegs = segments.slice(answeredIdx.current);
      if (newSegs.length === 0) return;
      const latest = transcriptText(newSegs).trim();
      if (latest.length < 2) return;
      // Card header = the interviewer's words only. The user's stutter-y "You:" lines are
      // context for the model (still sent in `latest`), not the question being answered, so
      // showing only the "Them:" text keeps the header short and readable.
      const themOnly = newSegs
        .filter((s) => s.role === "them")
        .map((s) => s.text)
        .join(" ")
        .trim();
      // A pure acknowledgement / filler turn ("ok", "yeah", "mhmm", "right") is not a question.
      // Consume it (advance the cursor so it still serves as context for later answers) but
      // never spend a round-trip or post a card - this is the main thing that broke the
      // assistant when the interviewer's English was halting and the ASR chopped it up.
      if (isBackchannel(themOnly)) {
        answeredIdx.current = segments.length;
        return;
      }
      const displayQ = themOnly || latest;
      const ctx = transcriptText(segments.slice(0, answeredIdx.current));
      answeredIdx.current = segments.length;
      void (async () => {
        const id = ++idRef.current;
        let acc = "";
        let created = false;
        const channel = new Channel<api.AnswerChunk>();
        channel.onmessage = (msg) => {
          if (msg.kind === "delta") {
            acc += msg.text;
            if (!created) {
              if (looksNone(acc)) return; // still ambiguous (N / NO / NON / NONE)
              created = true;
              setQa((prev) => [{ id, q: displayQ, a: acc, pending: true }, ...prev]);
            } else {
              setQa((prev) => prev.map((e) => (e.id === id ? { ...e, a: acc, pending: true } : e)));
            }
          } else if (msg.kind === "done") {
            const full = (msg.text || acc).trim();
            // isNoneAnswer ignores any trailing "Next:" hint, so a stray hint can never make a
            // filler reply look like a real answer (see lib/answer).
            if (isNoneAnswer(full)) {
              if (created) setQa((prev) => prev.filter((e) => e.id !== id)); // retract a NONE
              return;
            }
            if (!created) {
              created = true;
              setQa((prev) => [{ id, q: displayQ, a: full, pending: false }, ...prev]);
            } else {
              setQa((prev) => prev.map((e) => (e.id === id ? { ...e, a: full, pending: false } : e)));
            }
          } else if (created) {
            setQa((prev) =>
              prev.map((e) => (e.id === id ? { ...e, error: msg.message, pending: false } : e)),
            );
          }
          // delta/done errors before a bubble exists are dropped so the loop keeps going.
        };
        try {
          await api.autoAnswer(ctx, latest, channel);
        } catch {
          /* ignore transient auto-answer errors so the loop keeps going */
        }
      })();
    };
    // Adaptive debounce so we answer the WHOLE question, not a half-delivered one. Deepgram
    // ends an utterance after ~0.5s of silence (endpointing=500), so a long question lands as
    // several "Them:" segments with pauses between them. If the interviewer's line is the
    // newest, wait for the rest of it before answering - but if it already ends in a question
    // mark she is likely done, so answer sooner. Once the candidate has started replying (the
    // newest line is "You:"), the question is complete, so fire quickly.
    const lastSeg = pending[pending.length - 1];
    const themTail = lastSeg?.role === "them" ? lastSeg.text.trim() : "";
    const waitMs = themTail ? (/[?!]\s*$/.test(themTail) ? 800 : 1400) : 400;
    const t = setTimeout(fire, waitMs);
    return () => clearTimeout(t);
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

  // Toggle whether the floating capture panels are hidden from screen capture. Applies
  // live to any open panels, so the user can flip it on to screenshot them and back off
  // before sharing their screen.
  const changeCaptureProtect = async (on: boolean) => {
    setCaptureProtect(on);
    await api.setCaptureProtection(on).catch(() => {});
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
        setClaudeSpentInput(u.claudeSpentOverride != null ? String(u.claudeSpentOverride) : "");
        setDgSpentInput(u.deepgramSpentOverride != null ? String(u.deepgramSpentOverride) : "");
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

  // Set the manual "actual spent" override so the local estimate can be corrected to
  // match the provider console. An empty field clears the override (back to the
  // estimate); a typed negative or unparseable value is ignored, not treated as a clear.
  const saveSpent = async (provider: "claude" | "deepgram", raw: string) => {
    if (raw.trim() === "") {
      await api.setUsageSpent(provider, -1); // empty -> clear override
    } else {
      const n = Number.parseFloat(raw);
      if (Number.isNaN(n) || n < 0) return; // reject negative / unparseable
      await api.setUsageSpent(provider, n);
    }
    setUsage(await api.getUsage());
  };

  const claudeCost = (usage?.claudeItems ?? []).reduce((s, it) => s + estimateCost(it), 0);
  const claudeTokens = (usage?.claudeItems ?? []).reduce((s, it) => s + it.input + it.output, 0);
  const dgMinutes = (usage?.deepgramSeconds ?? 0) / 60;
  const dgCost = dgMinutes * DEEPGRAM_PER_MIN;
  // Effective spend shown to the user: the manual override when set, else the estimate.
  const claudeUsed = usage?.claudeSpentOverride ?? claudeCost;
  const dgUsed = usage?.deepgramSpentOverride ?? dgCost;
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
    stickToBottom.current = true; // re-pin so a new session follows the newest line
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
    stickToBottom.current = true; // a fresh dialog should follow the newest line again
  };

  // Start fresh between interviews/questions: drop the answers, the read-in page/notes context,
  // and the transcript so nothing from a previous session leaks into the next answer.
  const newSession = () => {
    setQa([]);
    setContext("");
    contextRef.current = "";
    clearTranscript();
  };

  // Build the saved-session text (transcript + answered Q&A, oldest first).
  const buildSession = (segs: DialogSegment[], qaList: QA[]) => {
    const transcript = transcriptText(segs);
    const answers = qaList
      .filter((e) => e.a && !e.pending)
      .slice()
      .reverse()
      .map((e) => `Q: ${e.q}\nA: ${e.a}`)
      .join("\n\n---\n\n");
    return { transcript, answers };
  };

  // Persist the current transcript + answers as a reviewable session (manual button).
  const saveSession = async () => {
    const { transcript, answers } = buildSession(segments, qa);
    if (!transcript.trim() && !answers.trim()) return;
    try {
      await api.saveSession(`Session ${new Date().toLocaleString()}`, transcript, answers);
      sessionSavedRef.current = true;
      setSessionsReload((n) => n + 1);
      setSessionSaved(true);
      setTimeout(() => setSessionSaved(false), 1800);
    } catch {
      /* ignore save errors */
    }
  };

  // Auto-save the session when listening stops, so an interview is never lost if the
  // user forgets to save (or the capture drops). Reads the refs because the stop
  // flush commits asynchronously; saves at most once per listening session.
  const autoSaveSession = async () => {
    if (sessionSavedRef.current) return;
    const { transcript, answers } = buildSession(segmentsRef.current, qaRef.current);
    if (!transcript.trim() && !answers.trim()) return;
    sessionSavedRef.current = true;
    try {
      await api.saveSession(`Auto-saved ${new Date().toLocaleString()}`, transcript, answers);
      setSessionsReload((n) => n + 1);
      setSessionSaved(true);
      setTimeout(() => setSessionSaved(false), 2500);
    } catch {
      /* ignore auto-save errors */
    }
  };

  // Watch the listening flag: a new session clears the saved marker; when listening
  // stops, auto-save after a short delay so the final stop flush lands first.
  useEffect(() => {
    if (listening) {
      sessionSavedRef.current = false;
      wasListeningRef.current = true;
      return;
    }
    if (!wasListeningRef.current) return;
    wasListeningRef.current = false;
    const t = setTimeout(() => void autoSaveSession(), 600);
    return () => clearTimeout(t);
  }, [listening]);

  const askManual = () => {
    const ctx = context.trim() || transcriptText(segments);
    if (askImage) {
      void askImageQ(question, askImage, ctx);
      setQuestion("");
      setAskImage(null);
      return;
    }
    if (!question.trim()) return;
    void ask(question, ctx);
    setQuestion("");
  };

  // --- Phone companion -------------------------------------------------------
  const startPhone = async () => {
    setPhoneBusy(true);
    setPhoneErr("");
    try {
      setPhone(await api.startPhoneServer());
      // Surface a firewall/network warning up front, rather than after the user fails to
      // connect from their phone and blames the app. Non-blocking - the server is already up.
      void api
        .phoneNetworkStatus()
        .then(setPhoneNet)
        .catch(() => setPhoneNet(null));
    } catch (e) {
      setPhoneErr(String(e));
    } finally {
      setPhoneBusy(false);
    }
  };
  const stopPhone = async () => {
    try {
      await api.stopPhoneServer();
    } catch {
      /* ignore */
    }
    setPhone(null);
    setPhoneNet(null);
  };
  // Marks the network Private and opens the companion port to the local subnet only.
  // Triggers a Windows UAC prompt; on success the refreshed status clears the warning.
  const allowFirewall = async () => {
    setFixingFw(true);
    setPhoneErr("");
    try {
      setPhoneNet(await api.phoneAllowFirewall());
    } catch (e) {
      setPhoneErr(String(e));
    } finally {
      setFixingFw(false);
    }
  };

  // Commands arriving from a phone are dispatched through this ref, reassigned every
  // render so the once-registered listener always invokes the latest handlers.
  const phoneCmdRef = useRef<
    (c: {
      cmd?: string;
      text?: string;
      dir?: string;
      monitor?: number;
      panel?: string;
      snap?: string;
      nudge?: string;
      resize?: string;
    }) => void
  >(() => {});
  phoneCmdRef.current = (c) => {
    switch (c.cmd) {
      case "listen":
        void toggleListen();
        break;
      case "opacity": {
        // Step the whole-window opacity from the phone, clamped to the same 0.3..1 range
        // as the desktop slider. The existing effect applies it to the window.
        const step = c.dir === "down" ? -0.1 : 0.1;
        setOpacity((o) => Math.min(1, Math.max(0.3, Math.round((o + step) * 10) / 10)));
        break;
      }
      case "answer_screen":
        // A monitor index from the phone targets that display; otherwise the UI's choice.
        // Phone Read & Answer is hybrid: full page text (scroll-independent) + screenshot.
        void runScreen(true, typeof c.monitor === "number" ? c.monitor : undefined, true);
        break;
      case "ask":
        if (c.text) void ask(c.text, transcriptText(segments));
        break;
      // Capture-mode control, so the whole flow can be driven from the phone.
      case "capture":
        captureOn ? void stopCapture() : void startCapture();
        break;
      case "panels":
        // Only meaningful once panels exist; a no-op otherwise.
        if (captureOn) void api.toggleCapturePanels().catch(() => {});
        break;
      case "move_panel":
        // Move or resize a panel (or "all") from the phone: snap, nudge, or grow/shrink.
        if (captureOn) void api.moveCapturePanel(c.panel || "all", c.snap, c.nudge, c.resize).catch(() => {});
        break;
      case "read_full_page":
        // Read the whole page (incl. scrolled-off text) into context. Focus-safe: the phone
        // doesn't change PC focus, and the read targets the last external window regardless.
        void readFullPage();
        break;
      case "watch":
        setWatchingScreen((v) => !v);
        break;
      case "protect":
        // Gate on captureOn so a stray phone tap can't flip the persisted
        // screen-capture-protection default while no panels are open (which would make
        // the next capture session open exposed to a screen share).
        if (captureOn) void changeCaptureProtect(!captureProtect);
        break;
    }
  };

  // --- Capture mode -----------------------------------------------------------
  // The snapshot mirrored to the overlay panels: committed dialog plus any
  // in-progress live line, the answer feed, and the two live toggles.
  const captureState = useCallback((): CaptureState => {
    const segs: CaptureSegment[] = segments.map((s) => ({ role: s.role, speaker: s.speaker, text: s.text }));
    (["them", "you"] as const).forEach((role) => {
      const slot = live[role];
      const txt = (slot.fin + " " + slot.int).trim();
      if (txt) segs.push({ role, speaker: role === "them" ? "Them" : "You", text: txt });
    });
    return {
      listening,
      watching: watchingScreen,
      protect: captureProtect,
      segments: segs,
      answers: qa.map((e) => ({ q: e.q, a: e.a, pending: e.pending })),
      opacity,
      log: logEntries.slice(0, 40),
    };
  }, [listening, watchingScreen, captureProtect, segments, live, qa, opacity, logEntries]);
  const pushCaptureState = useCallback(() => void emit("capture-state", captureState()), [captureState]);

  const startCapture = async () => {
    if (captureBusy.current) return;
    captureBusy.current = true;
    setCaptureErr("");
    try {
      await api.openCapturePanels(screenMonitor); // open on the chosen display
      setCaptureOn(true);
      // Don't auto-start listening - open quietly and let the user press Listen (on the
      // control bar or the Controls panel) when ready.
    } catch (e) {
      setCaptureErr(String(e));
    } finally {
      captureBusy.current = false;
    }
  };
  const stopCapture = async () => {
    if (captureBusy.current) return;
    captureBusy.current = true;
    try {
      await api.closeCapturePanels();
    } catch {
      /* ignore */
    } finally {
      // Flip the flag after the close resolves so it tracks the real window state.
      setCaptureOn(false);
      captureBusy.current = false;
    }
  };

  // Dispatch commands arriving from the overlay panels. Reassigned every render so
  // the once-registered listener always calls the latest handlers.
  const captureCmdRef = useRef<(c: CaptureCommand) => void>(() => {});
  captureCmdRef.current = (c) => {
    switch (c.cmd) {
      case "listen":
        void toggleListen();
        break;
      case "watch":
        setWatchingScreen((v) => !v);
        break;
      case "snip":
        void startSnip();
        break;
      case "answer_screen":
        void runScreen(true);
        break;
      case "read_page":
        void readPage();
        break;
      case "read_full_page":
        void readFullPage();
        break;
      case "opacity":
        // Route through the shared opacity state so the app slider, the phone, and the panels
        // all move together (the effect applies it to both the window and the capture panels).
        if (typeof c.value === "number") setOpacity(Math.min(1, Math.max(0.3, c.value)));
        break;
      case "ask":
        if (c.text) void ask(c.text, transcriptText(segments));
        break;
      case "protect":
        void changeCaptureProtect(!captureProtect);
        break;
      case "stop":
        void stopCapture();
        break;
      case "sync":
        pushCaptureState();
        break;
    }
  };
  useEffect(() => {
    let cancelled = false;
    const uns: UnlistenFn[] = [];
    const add = (p: Promise<UnlistenFn>) => void p.then((u) => (cancelled ? u() : uns.push(u)));
    add(
      listen<string>("phone-command", (e) => {
        try {
          phoneCmdRef.current(JSON.parse(String(e.payload)));
        } catch {
          /* ignore malformed command */
        }
      }),
    );
    // Global keyboard shortcuts arrive as a command object and dispatch like a phone tap.
    add(
      listen<{ cmd?: string; dir?: string }>("hotkey", (e) => {
        const c = e.payload || {};
        pushLog("shortcut", `Hotkey: ${c.cmd ?? "?"}${c.dir ? " " + c.dir : ""}`);
        phoneCmdRef.current(c);
      }),
    );
    add(listen<CaptureCommand>("capture-command", (e) => captureCmdRef.current(e.payload)));
    return () => {
      cancelled = true;
      uns.forEach((u) => u());
    };
  }, []);

  // The JSON state snapshot mirrored to connected phones: committed dialog plus any
  // in-progress live line, and the answer feed.
  const phoneState = useCallback(() => {
    const segs = segments.map((s) => ({ role: s.role, speaker: s.speaker, text: s.text }));
    (["them", "you"] as const).forEach((role) => {
      const slot = live[role];
      const txt = (slot.fin + " " + slot.int).trim();
      if (txt) segs.push({ role, speaker: role === "them" ? "Them" : "You", text: txt });
    });
    return JSON.stringify({
      type: "state",
      listening,
      watching: watchingScreen,
      capture: captureOn,
      protect: captureProtect,
      segments: segs,
      answers: qa.map((e) => ({ q: e.q, a: e.a, pending: e.pending })),
      monitors: monitors.length,
    });
  }, [listening, watchingScreen, captureOn, captureProtect, segments, live, qa, monitors]);

  // Push state to phones on change (debounced) plus a heartbeat so a phone that
  // connects mid-session catches up within a couple of seconds.
  useEffect(() => {
    if (!phone) return;
    const push = () => void api.phoneBroadcast(phoneState()).catch(() => {});
    const t = setTimeout(push, 150);
    const hb = setInterval(push, 2500);
    return () => {
      clearTimeout(t);
      clearInterval(hb);
    };
  }, [phone, phoneState]);

  // Push state to the panels on change (debounced) plus a heartbeat so a panel
  // that just opened catches up even with no further changes.
  useEffect(() => {
    if (!captureOn) return;
    const t = setTimeout(pushCaptureState, 120);
    const hb = setInterval(pushCaptureState, 1500);
    return () => {
      clearTimeout(t);
      clearInterval(hb);
    };
  }, [captureOn, pushCaptureState]);

  // Never leave overlay panels floating when this view unmounts (tab switch / close).
  useEffect(() => () => void api.closeCapturePanels().catch(() => {}), []);

  const needsDeepgramKey = sttEngine === "deepgram" && !hasSttKey;
  // snippetGroups skips pending cards, so its output only changes when an answer
  // finishes - key the memo on the pending-state vector + count instead of `qa`,
  // so we don't re-scan every answer's regex on every streamed token.
  const cmKey = qa.map((e) => (e.pending ? "1" : "0")).join("") + "|" + qa.length;
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const cmGroups = useMemo(() => snippetGroups(qa), [cmKey]);
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
              {/* Drawer header - only exists in the narrow layout where Setup is a drawer.
              In the wide layout Setup is a permanent column, so there's nothing to close. */}
              {narrow && (
                <div className="drawer-head">
                  <span>Setup</span>
                  <button className="icon-btn sm" title="Close" onClick={() => setSettingsOpen(false)}>
                    <X size={16} />
                  </button>
                </div>
              )}
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
                    <button
                      className="btn-open"
                      onClick={saveSttKey}
                      disabled={savingStt || !sttKeyInput.trim()}
                    >
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
                  <button
                    className={"btn-listen" + (captureOn ? " on" : "")}
                    onClick={() => (captureOn ? void stopCapture() : void startCapture())}
                    title="Pop out floating, always-on-top panels you can drag over your call (starts listening)"
                  >
                    {captureOn ? <Square size={15} /> : <LayoutDashboard size={15} />}
                    {captureOn ? "Exit capture" : "Capture mode"}
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

              {captureErr && <div className="assistant-error">{captureErr}</div>}
              {captureOn && (
                <>
                  <div className="muted sm">
                    A control bar sits at the top of your screen (listen, screenshot, show/hide). Panels are
                    floating and always-on-top - drag them anywhere. Press{" "}
                    <span className="kbd">Alt+Shift+C</span> to hide or show the panels instantly.
                  </div>
                  <label className="auto-toggle muted sm">
                    <input
                      type="checkbox"
                      checked={!captureProtect}
                      onChange={(e) => void changeCaptureProtect(!e.target.checked)}
                    />
                    Show panels in screenshots / screen share
                  </label>
                  {!captureProtect && (
                    <div className="assistant-warn sm">
                      Panels are now visible to screen capture, so you can screenshot them - but anyone you
                      share your screen with will see them too. Turn this off again before the interview.
                    </div>
                  )}
                </>
              )}
              {listenError && <div className="assistant-error">{listenError}</div>}

              {listening && (
                <div className={"capture-diag" + (Math.max(level.mic, level.sys) < 0.005 ? " warn" : " ok")}>
                  <div className="capture-diag-head">
                    <span className="muted sm">Capture diagnostics</span>
                    <span className="muted sm">{Math.round(level.buffered / 1000)}k buffered</span>
                  </div>
                  <div className="level-row">
                    <span className="level-label">You (mic)</span>
                    <div className="level-track">
                      <div
                        className={"level-fill" + (level.mic >= 0.005 ? " live" : "")}
                        style={{ width: `${Math.min(100, Math.round(level.mic * 140))}%` }}
                      />
                    </div>
                  </div>
                  <div className="level-row">
                    <span className="level-label">Them (output)</span>
                    <div className="level-track">
                      <div
                        className={"level-fill" + (level.sys >= 0.005 ? " live" : "")}
                        style={{ width: `${Math.min(100, Math.round(level.sys * 140))}%` }}
                      />
                    </div>
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
                    {level.sys < 0.005 && level.mic < 0.005
                      ? "No audio on either source. Check the capture devices below match what's playing your call."
                      : level.sys < 0.005
                        ? "Your mic is captured, but no other-side audio. In Windows Sound settings (Volume mixer), make sure the call app - browser, Teams, etc. - is set to output to the device shown above."
                        : "Audio is being captured."}
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
                <button
                  className="btn-ghost"
                  onClick={() => void startSnip()}
                  disabled={watchingScreen}
                  title="Snip a region of the screen and explain it"
                >
                  <Crop size={14} /> Snip
                </button>
                <button
                  className="btn-ghost"
                  onClick={() => void readFullPage()}
                  title="Read the WHOLE focused page automatically via accessibility - including content scrolled off-screen - and attach it as context. No copy needed; it reads the window you were last on, dedupes, and accumulates (so it never duplicates answers or loses context)."
                >
                  <ScrollText size={14} /> Read full page
                </button>
                <button
                  className="btn-ghost"
                  onClick={() => void readPage()}
                  title="Manual fallback: copy the page in your browser (Ctrl+A, Ctrl+C) then click to attach it as context. On a coding page, also copy the editor's code and click again to add it. Each click appends; use Clear to reset."
                >
                  <ClipboardPaste size={14} /> Read page (clipboard)
                </button>
              </div>
              <label
                className="auto-toggle muted sm"
                title="When on, reading a page also answers it (works in a live call too). Turn off to only stack the page as context, e.g. read several parts then Ask once."
              >
                <input
                  type="checkbox"
                  checked={solveOnRead}
                  onChange={(e) => setSolveOnRead(e.target.checked)}
                />
                Solve on read (answer the page, even during a call)
              </label>
              {pageMsg && <div className="muted sm">{pageMsg}</div>}
              <div className="muted sm">
                <b>Read full page</b> reads the whole page (incl. off-screen) automatically - good for
                proctored / locked pages. <span className="kbd">Alt+Shift+R</span> does the same while the
                page is focused (Windows).
              </div>
              {screenError && <div className="assistant-error">{screenError}</div>}
              {screenNote && !screenError && <div className="muted sm">{screenNote}</div>}

              <details className="acc" open>
                <summary>
                  <SlidersHorizontal size={15} /> Setup
                </summary>
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
                          title="Splits only the other side (your system/loopback audio) into Speaker 1/2/... for calls with several people on the far end. It can't tell you apart from them - that's set by the capture source (your mic is always 'You'). Leave off for 1-on-1 calls; it tends to mislabel one person as several."
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
                      {sttDiarize && (
                        <div className="setup-row diarize-hint muted sm">
                          Only splits the other side into Speaker 1/2 (for 3+ people). You vs. them is set by
                          the capture source - your mic is always &quot;You&quot;. If your own voice also
                          shows up here as a speaker, it's leaking into the system audio: use headphones (so
                          it isn't re-captured) and turn off any mic monitoring / &quot;Stereo Mix&quot;.
                        </div>
                      )}
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
                <summary>
                  <Monitor size={15} /> Overlay
                </summary>
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
                  <label className="auto-toggle muted sm">
                    <input
                      type="checkbox"
                      checked={skipTaskbar}
                      onChange={(e) => setSkipTaskbar(e.target.checked)}
                    />
                    Hide from taskbar &amp; Alt+Tab
                  </label>
                  <div className="muted sm">
                    No taskbar button or Alt+Tab entry while on; reach the app from its tray icon or{" "}
                    <span className="kbd">Alt+Shift+H</span>.
                  </div>
                  <div className="muted sm">Dims the whole window (this UI too); min 30%.</div>
                  <div className="muted sm">
                    Panic hide: press <span className="kbd">Alt+Shift+H</span> anytime to hide/show the window
                    instantly (Windows).
                  </div>
                </div>
              </details>

              <details className="acc">
                <summary>
                  <ScrollText size={15} /> Activity log
                </summary>
                <div className="acc-body">
                  <div className="muted sm">
                    What each shortcut/action did. Watch = re-reads the screen and answers; Read page / window
                    = attaches the page text as context (no answer).
                  </div>
                  {logEntries.length === 0 ? (
                    <div className="muted sm">No activity yet - try a shortcut or screen action.</div>
                  ) : (
                    <>
                      <div className="act-log">
                        {logEntries.map((l) => (
                          <div className={"act-log-row k-" + l.kind} key={l.id}>
                            <span className="act-log-time">{fmtLogClock(l.ts)}</span>
                            <span className="act-log-kind">{l.kind}</span>
                            <span className="act-log-msg">{l.msg}</span>
                          </div>
                        ))}
                      </div>
                      <button className="btn-ghost sm" onClick={() => setLogEntries([])}>
                        Clear log
                      </button>
                    </>
                  )}
                </div>
              </details>

              <details className="acc">
                <summary>
                  <Smartphone size={15} /> Phone control
                </summary>
                <div className="acc-body">
                  {!phone ? (
                    <>
                      <div className="muted sm">
                        Run a server on your Wi-Fi so your phone can watch the transcript and answers and
                        drive listening - a discreet second screen during a call.
                      </div>
                      <button className="btn-ghost" onClick={() => void startPhone()} disabled={phoneBusy}>
                        <Smartphone size={14} /> {phoneBusy ? "Starting…" : "Start phone server"}
                      </button>
                    </>
                  ) : (
                    <div className="phone-conn">
                      {phone.qr && (
                        <img
                          className="phone-qr"
                          src={phone.qr}
                          alt="Scan to connect"
                          width={150}
                          height={150}
                        />
                      )}
                      <div className="phone-conn-info">
                        <div className="phone-row">
                          <span className="setup-label">Open</span>
                          <code className="phone-url">{phone.url}</code>
                        </div>
                        <div className="phone-row">
                          <span className="setup-label">PIN</span>
                          <span className="phone-pin">{phone.pin}</span>
                        </div>
                        <button className="btn-ghost" onClick={() => void stopPhone()}>
                          <Square size={14} /> Stop server
                        </button>
                      </div>
                    </div>
                  )}
                  {phone && phoneNet && !phoneNet.ok && (
                    <div className="phone-fw-warn">
                      <div className="muted sm">{phoneNet.message}</div>
                      {phoneNet.canFix && (
                        <button
                          className="btn-ghost"
                          onClick={() => void allowFirewall()}
                          disabled={fixingFw}
                        >
                          <ShieldCheck size={14} /> {fixingFw ? "Allowing…" : "Allow through firewall"}
                        </button>
                      )}
                    </div>
                  )}
                  {phoneErr && <div className="assistant-error">{phoneErr}</div>}
                  <div className="muted sm">
                    Scan the code to connect instantly, or open the URL and type the PIN. Same Wi-Fi only; the
                    code carries the PIN and traffic is unencrypted, so use a network you trust.
                  </div>
                </div>
              </details>

              <details className="acc">
                <summary>
                  <Briefcase size={15} /> Interview context
                </summary>
                <div className="acc-body">
                  <p className="muted sm">Fed to the assistant so answers fit you and the role.</p>
                  {profiles.length > 0 && (
                    <div className="setup-field">
                      <span className="setup-label">Active profile</span>
                      <select
                        value={activeProfileId ?? ""}
                        onChange={(e) => void switchProfile(Number(e.target.value))}
                      >
                        {profiles.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name || "Untitled"}
                          </option>
                        ))}
                      </select>
                      <div className="muted sm">Manage all profiles from the person icon in the top bar.</div>
                    </div>
                  )}
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
                      placeholder="Paste your resume, or import a file below…"
                    />
                    <div className="resume-import">
                      <input
                        ref={resumeFileRef}
                        type="file"
                        accept=".pdf,.docx,.txt,.md"
                        className="hidden-file"
                        onChange={(e) => void onResumeFile(e)}
                      />
                      <button className="link-btn" onClick={() => resumeFileRef.current?.click()}>
                        <Upload size={13} /> Import PDF / DOCX / text
                      </button>
                      {resumeMsg && <span className="muted sm">{resumeMsg}</span>}
                    </div>
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
                <summary>
                  <MessageSquare size={15} /> Your stories
                </summary>
                <div className="acc-body">
                  <StoriesPanel key={activeProfileId ?? "none"} />
                </div>
              </details>

              <details className="acc">
                <summary>
                  <History size={15} /> Past sessions
                </summary>
                <div className="acc-body">
                  <SessionsPanel reload={sessionsReload} />
                </div>
              </details>

              <details className="acc">
                <summary>
                  <Building2 size={15} /> Company information
                </summary>
                <div className="acc-body">
                  <p className="muted sm">
                    Add the company you're interviewing at - answers become company-aware, and you can
                    generate a briefing + questions to ask.
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
                <summary>
                  <FileText size={15} /> Application kit
                </summary>
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
                <summary>
                  <Gauge size={15} /> Usage &amp; cost
                </summary>
                <div className="acc-body">
                  {/* Claude: token-based spend estimate, optional credit -> remaining. */}
                  <div className="usage-row">
                    <span className="usage-name">Claude</span>
                    <span className="usage-val">
                      {usage && usage.claudeCredit > 0
                        ? `$${Math.max(0, usage.claudeCredit - claudeUsed).toFixed(2)} left`
                        : `$${claudeUsed.toFixed(2)} used`}
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
                      {modelLabel(it.model)}: {(it.input / 1000).toFixed(0)}k in /{" "}
                      {(it.output / 1000).toFixed(0)}k out
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
                  <label className="usage-credit">
                    <span className="setup-label">Actual spent ($)</span>
                    <input
                      type="number"
                      min={0}
                      step="0.01"
                      placeholder="sync to console"
                      title="Override the estimate with the real spend from your Anthropic console. Leave blank to use the estimate."
                      value={claudeSpentInput}
                      onChange={(e) => setClaudeSpentInput(e.target.value)}
                      onBlur={() => void saveSpent("claude", claudeSpentInput)}
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
                            ? `$${Math.max(0, usage.deepgramCredit - dgUsed).toFixed(2)} left`
                            : `$${dgUsed.toFixed(2)} used`}
                    </span>
                  </div>
                  {dgBalance ? (
                    <div className="muted sm">
                      Live balance{dgBalance.project ? ` · ${dgBalance.project}` : ""}
                    </div>
                  ) : (
                    <div className="muted sm">
                      {`~$${dgCost.toFixed(2)} est. (${dgMinutes.toFixed(1)} min streamed)`}
                      {dgError ? " · live balance unavailable for this key" : ""}
                    </div>
                  )}
                  {!dgBalance && (
                    <>
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
                      <label className="usage-credit">
                        <span className="setup-label">Actual spent ($)</span>
                        <input
                          type="number"
                          min={0}
                          step="0.01"
                          placeholder="sync to console"
                          title="Override the estimate with the real spend from your Deepgram console. Leave blank to use the estimate."
                          value={dgSpentInput}
                          onChange={(e) => setDgSpentInput(e.target.value)}
                          onBlur={() => void saveSpent("deepgram", dgSpentInput)}
                        />
                      </label>
                    </>
                  )}
                  <div className="usage-actions">
                    <button className="link-btn" onClick={() => void refreshUsage()} disabled={dgLoading}>
                      Refresh
                    </button>
                    <button
                      className="link-btn"
                      onClick={() => void openUrl("https://console.deepgram.com/")}
                    >
                      Open Deepgram console
                    </button>
                  </div>
                  {dgError && (
                    <div className="muted sm">
                      Live balance needs an Owner/Admin Deepgram key; a transcription key can't read it. Enter
                      your credit above for an estimate, or check the console.
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
                    <button
                      className="link-btn"
                      onClick={() => void saveSession()}
                      title="Sessions also auto-save when you stop listening"
                    >
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
                <div
                  className="dialog-list"
                  aria-live="polite"
                  ref={dialogListRef}
                  onScroll={() => {
                    const el = dialogListRef.current;
                    if (el) stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
                  }}
                >
                  {segments.map((s, i) => {
                    const prev = segments[i - 1];
                    // Gap since the other speaker's previous line = how long this turn took to start.
                    const gap = prev && prev.role !== s.role && prev.ts && s.ts ? s.ts - prev.ts : null;
                    return (
                      <div key={s.id} className={"seg-msg seg-" + s.role}>
                        <div className="seg-head">
                          {s.role === "them" && (
                            <span className={"seg-chip " + chipClass(s.speaker)}>{s.speaker}</span>
                          )}
                          {s.ts ? <span className="seg-time">{fmtSegTime(s.ts)}</span> : null}
                          {gap != null && (
                            <span className="seg-gap" title="Time since the other speaker's last line">
                              +{(gap / 1000).toFixed(2)}s
                            </span>
                          )}
                        </div>
                        <div className="seg-bubble">{s.text}</div>
                      </div>
                    );
                  })}
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
                    Hit <strong>Listen to call</strong> - the conversation appears here, you on the right, the
                    other side on the left.
                  </span>
                </div>
              )}

              {askImage && (
                <div className="ask-image">
                  <div className="ask-image-thumb">
                    <img src={"data:image/png;base64," + askImage} alt="attached" />
                    <button
                      className="ask-image-x"
                      title="Remove image"
                      aria-label="Remove image"
                      onClick={() => setAskImage(null)}
                    >
                      <X size={14} />
                    </button>
                  </div>
                  <button className="ask-image-remove" onClick={() => setAskImage(null)}>
                    <X size={13} /> Remove image
                  </button>
                </div>
              )}
              <div
                className="assistant-ask"
                onDragOver={(e) => {
                  if (Array.from(e.dataTransfer.types).includes("Files")) e.preventDefault();
                }}
                onDrop={(e) => {
                  const f = Array.from(e.dataTransfer.files).find((x) => x.type.startsWith("image/"));
                  if (f) {
                    e.preventDefault();
                    void imageToPngBase64(f)
                      .then(setAskImage)
                      .catch(() => {});
                  }
                }}
              >
                <textarea
                  placeholder="Ask directly, or paste / drop a screenshot - e.g. 'Solve 3x + 7 = 22'"
                  value={question}
                  onChange={(e) => setQuestion(e.target.value)}
                  onPaste={(e) => {
                    const item = Array.from(e.clipboardData.items).find((i) => i.type.startsWith("image/"));
                    const blob = item?.getAsFile();
                    if (blob) {
                      e.preventDefault();
                      void imageToPngBase64(blob)
                        .then(setAskImage)
                        .catch(() => {});
                    }
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) askManual();
                  }}
                  rows={2}
                />
                <input
                  ref={imgInputRef}
                  type="file"
                  accept="image/*"
                  hidden
                  onChange={(e) => {
                    const f = e.target.files?.[0];
                    if (f)
                      void imageToPngBase64(f)
                        .then(setAskImage)
                        .catch(() => {});
                    e.target.value = "";
                  }}
                />
                <button
                  className="icon-btn ask-attach"
                  title="Attach an image (or paste / drop one)"
                  onClick={() => imgInputRef.current?.click()}
                >
                  <ImagePlus size={16} />
                </button>
                <button className="btn-sync" onClick={askManual} disabled={!question.trim() && !askImage}>
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
                <div className="pane-title">
                  <span>Live answers</span>
                  {(qa.length > 0 || context.length > 0 || segments.length > 0) && (
                    <span className="pane-actions">
                      {qa.length > 0 && (
                        <button
                          className="link-btn"
                          onClick={() => void saveSession()}
                          title="Save the transcript + these answers as a reviewable session (also auto-saves when you stop listening)"
                        >
                          {sessionSaved ? "Saved ✓" : "Save"}
                        </button>
                      )}
                      {qa.length > 0 && (
                        <button
                          className="link-btn"
                          onClick={() => setQa([])}
                          title="Clear just these answers (keeps the read-in context and the transcript)"
                        >
                          Clear
                        </button>
                      )}
                      <button
                        className="link-btn"
                        onClick={newSession}
                        title="Start fresh: clears the answers, the read-in page/notes context, and the transcript so nothing from a previous interview or question carries into the next answer"
                      >
                        New session
                      </button>
                    </span>
                  )}
                </div>
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
                      <span className="muted sm">
                        Code blocks &amp; equations pulled from answers collect here.
                      </span>
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

      {snipSrc && <SnipOverlay src={snipSrc} onCancel={() => setSnipSrc(null)} onSelect={onSnipSelect} />}
    </div>
  );
}
