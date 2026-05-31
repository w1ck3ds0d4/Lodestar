import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Bot, Ear, KeyRound, Monitor, ScanSearch, Send, Square } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import * as api from "../api";
import { ASSISTANT_MODELS, type AudioDevices, type MonitorInfo } from "../types";

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

function QaCard({ e }: { e: QA }) {
  return (
    <div className="qa">
      <div className="qa-q">{e.q}</div>
      {e.pending ? (
        <div className="muted sm">Thinking...</div>
      ) : e.error ? (
        <div className="assistant-error">{e.error}</div>
      ) : (
        <Answer text={e.a} />
      )}
    </div>
  );
}

export function AssistantView() {
  const [hasKey, setHasKey] = useState(false);
  const [hasSttKey, setHasSttKey] = useState(false);
  const [sttEngine, setSttEngine] = useState("deepgram");
  const [captureSource, setCaptureSource] = useState("system");
  const [devices, setDevices] = useState<AudioDevices>({ inputs: [], outputs: [] });
  const [inputDevice, setInputDevice] = useState("");
  const [outputDevice, setOutputDevice] = useState("");
  const [model, setModel] = useState("claude-opus-4-8");
  const [keyInput, setKeyInput] = useState("");
  const [sttKeyInput, setSttKeyInput] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [savingStt, setSavingStt] = useState(false);

  const [listening, setListening] = useState(false);
  const [listenStatus, setListenStatus] = useState("");
  const [segments, setSegments] = useState<DialogSegment[]>([]);
  const [listenError, setListenError] = useState("");
  const [autoAnswer, setAutoAnswer] = useState(true);
  const [captureInfo, setCaptureInfo] = useState<CaptureDevice[]>([]);
  const [level, setLevel] = useState<{ peak: number; buffered: number }>({ peak: 0, buffered: 0 });

  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [screenMonitor, setScreenMonitor] = useState(0);
  const [watchingScreen, setWatchingScreen] = useState(false);
  const [screenError, setScreenError] = useState("");
  const screenBusy = useRef(false);

  const [opacity, setOpacity] = useState(1);
  const [onTop, setOnTop] = useState(false);

  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [qa, setQa] = useState<QA[]>([]);

  const idRef = useRef(0);
  const segIdRef = useRef(0);
  const answeredIdx = useRef(0);

  // Overlay window controls. Applied after mount (hwnd() errors before the window
  // exists); opacity is debounced so dragging the slider doesn't spam invoke.
  useEffect(() => {
    const t = setTimeout(() => void api.setWindowOpacity(opacity).catch(() => {}), 60);
    return () => clearTimeout(t);
  }, [opacity]);
  useEffect(() => {
    void api.setWindowAlwaysOnTop(onTop).catch(() => {});
  }, [onTop]);

  // Build a speaker-prefixed transcript string from the dialog segments.
  const transcriptText = useCallback(
    (segs: DialogSegment[]) => segs.map((s) => `${s.speaker}: ${s.text}`).join("\n"),
    [],
  );

  const loadSettings = useCallback(async () => {
    const s = await api.getAssistantSettings();
    setHasKey(s.hasKey);
    setHasSttKey(s.hasSttKey);
    setSttEngine(s.sttEngine);
    setCaptureSource(s.captureSource);
    setInputDevice(s.inputDevice);
    setOutputDevice(s.outputDevice);
    setModel(s.model);
  }, []);
  useEffect(() => {
    void loadSettings();
    void api.listAudioDevices().then(setDevices).catch(() => {});
    void api.listMonitors().then(setMonitors).catch(() => {});
  }, [loadSettings]);

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

  // Auto-watch: poll the chosen screen while the toggle is on.
  useEffect(() => {
    if (!watchingScreen) return;
    void runScreen(false);
    const t = setInterval(() => void runScreen(false), 6000);
    return () => clearInterval(t);
  }, [watchingScreen, runScreen]);

  // Ask the assistant; result is appended to the Q&A feed (newest first).
  const ask = useCallback(async (q: string, ctx: string | null) => {
    const question = q.trim();
    if (!question) return;
    const id = ++idRef.current;
    setQa((prev) => [{ id, q: question, a: "", pending: true }, ...prev]);
    try {
      const a = await api.askAssistant(question, ctx && ctx.trim() ? ctx.trim() : null);
      setQa((prev) => prev.map((e) => (e.id === id ? { ...e, a, pending: false } : e)));
    } catch (e) {
      setQa((prev) => prev.map((e) => (e.id === id ? { ...e, error: String(e), pending: false } : e)));
    }
  }, []);

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
      listen<{ role: "you" | "them"; speaker: string; text: string }>("transcript", (e) =>
        setSegments((s) => [...s, { id: ++segIdRef.current, ...e.payload }]),
      ),
    );
    add(listen<string>("listen-status", (e) => setListenStatus(String(e.payload))));
    add(listen<string>("listen-error", (e) => setListenError(String(e.payload))));
    add(listen<CaptureDevice[]>("listen-device", (e) => setCaptureInfo(e.payload)));
    add(listen<{ peak: number; buffered: number }>("listen-level", (e) => setLevel(e.payload)));
    add(
      listen("listen-stopped", () => {
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
      try {
        const a = (await api.autoAnswer(ctx, latest)).trim();
        if (a && a.toUpperCase() !== "NONE") {
          setQa((prev) => [{ id: ++idRef.current, q: latest, a, pending: false }, ...prev]);
        }
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

  const changeEngine = async (e: string) => {
    setSttEngine(e);
    await api.setSttEngine(e);
  };

  const changeCaptureSource = async (s: string) => {
    setCaptureSource(s);
    await api.setCaptureSource(s);
  };

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
    try {
      await api.startListening();
      setListening(true);
    } catch (e) {
      setListenError(String(e));
    }
  };

  const clearTranscript = () => {
    setSegments([]);
    answeredIdx.current = 0;
  };

  const askManual = () => {
    if (!question.trim()) return;
    void ask(question, context.trim() || transcriptText(segments));
    setQuestion("");
  };

  const needsDeepgramKey = sttEngine === "deepgram" && !hasSttKey;
  const codeMathQa = qa.filter((e) => !e.pending && !e.error && hasCodeOrMath(e.a));

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

      {!hasKey ? (
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
        </div>
      ) : (
        <div className="assistant-body">
          <section className="assistant-col col-settings">
          <div className={"setup-card" + (listening ? " is-listening" : "")}>
            <div className="setup-title">Setup</div>

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
          </div>
          {screenError && <div className="assistant-error">{screenError}</div>}

          <div className="setup-card">
            <div className="setup-title">Overlay</div>
            <div className="overlay-row">
              <span className="setup-label">Opacity</span>
              <input
                type="range"
                min={0.3}
                max={1}
                step={0.05}
                value={opacity}
                onChange={(e) => setOpacity(Number(e.target.value))}
                title="Make the window translucent so it sits over a call (Windows)"
              />
              <span className="muted sm">{Math.round(opacity * 100)}%</span>
            </div>
            <label className="auto-toggle muted sm">
              <input type="checkbox" checked={onTop} onChange={(e) => setOnTop(e.target.checked)} />
              Always on top
            </label>
            <div className="muted sm">Dims the whole window (this UI too); min 30%.</div>
          </div>
          </section>

          <section className="assistant-col col-dialog">
            <div className="dialog-head">
              <span className="muted sm">{listening ? listenStatus || "Listening..." : "Live dialog"}</span>
              <span className="transcript-actions">
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

          {qa.length > 0 && (
            <details className="answers-history">
              <summary className="pane-title">All answers ({qa.length})</summary>
              {qa.map((e) => (
                <QaCard key={e.id} e={e} />
              ))}
            </details>
          )}
          </section>

          <aside className="assistant-col col-code">
            <div className="pane-title">Code &amp; Math</div>
            {codeMathQa.length === 0 ? (
              <div className="pane-empty">
                <strong>No snippets yet.</strong>
                <span className="muted sm">Code blocks and equations from answers collect here.</span>
              </div>
            ) : (
              codeMathQa.map((e) => <QaCard key={e.id} e={e} />)
            )}
          </aside>
        </div>
      )}
    </div>
  );
}
