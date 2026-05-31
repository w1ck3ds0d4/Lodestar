import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Bot, Ear, KeyRound, Send, Square } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import * as api from "../api";
import { ASSISTANT_MODELS, type AudioDevices } from "../types";

interface QA {
  id: number;
  q: string;
  a: string;
  pending: boolean;
  error?: string;
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
  const [transcript, setTranscript] = useState("");
  const [listenError, setListenError] = useState("");
  const [autoAnswer, setAutoAnswer] = useState(true);

  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [qa, setQa] = useState<QA[]>([]);

  const idRef = useRef(0);
  const answeredIdx = useRef(0);
  const transcriptRef = useRef<HTMLTextAreaElement>(null);

  // Keep the live transcript field pinned to the newest text.
  useEffect(() => {
    const el = transcriptRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [transcript]);

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
  }, [loadSettings]);

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
      listen<{ text: string }>("transcript", (e) =>
        setTranscript((t) => (t ? t + " " : "") + e.payload.text),
      ),
    );
    add(listen<string>("listen-status", (e) => setListenStatus(String(e.payload))));
    add(listen<string>("listen-error", (e) => setListenError(String(e.payload))));
    add(
      listen("listen-stopped", () => {
        setListening(false);
        setListenStatus("");
      }),
    );
    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
    };
  }, []);

  // Auto-answer: send each new transcript segment to the model, which answers it if
  // it is a question or problem (in any language) and replies "NONE" otherwise. This
  // replaces brittle keyword matching, so e.g. "um mais um" (Portuguese math) is caught.
  useEffect(() => {
    if (!autoAnswer || !listening) return;
    const latest = transcript.slice(answeredIdx.current).trim();
    if (latest.length < 2) return;
    const ctx = transcript.slice(0, answeredIdx.current);
    answeredIdx.current = transcript.length;
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
  }, [transcript, autoAnswer, listening]);

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
    setTranscript("");
    answeredIdx.current = 0;
  };

  const askManual = () => {
    if (!question.trim()) return;
    void ask(question, context.trim() || transcript);
    setQuestion("");
  };

  const needsDeepgramKey = sttEngine === "deepgram" && !hasSttKey;

  return (
    <div className="assistant">
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
        <>
          <div className="listen-engine">
            <span className="muted sm">Transcription:</span>
            <select value={sttEngine} onChange={(e) => void changeEngine(e.target.value)} disabled={listening}>
              <option value="deepgram">Cloud (Deepgram)</option>
              <option value="local">Local (Whisper, on-device)</option>
            </select>
          </div>

          <div className="listen-engine">
            <span className="muted sm">Capture:</span>
            <select
              value={captureSource}
              onChange={(e) => void changeCaptureSource(e.target.value)}
              disabled={listening}
            >
              <option value="system">System audio (the call/video)</option>
              <option value="mic">Microphone (you)</option>
              <option value="both">System + Mic (both sides)</option>
            </select>
          </div>

          {captureSource !== "mic" && (
            <div className="listen-engine">
              <span className="muted sm">Output device:</span>
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
            <div className="listen-engine">
              <span className="muted sm">Input device:</span>
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

          {needsDeepgramKey ? (
            <div className="add-source">
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
              <button className={"btn-listen" + (listening ? " on" : "")} onClick={toggleListen}>
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

          {(listening || transcript) && (
            <div className="transcript">
              <div className="transcript-head">
                <span className="muted sm">{listening ? listenStatus || "Listening..." : "Transcript"}</span>
                <span className="transcript-actions">
                  {transcript && (
                    <button className="link-btn" onClick={() => setContext(transcript)}>
                      Use as context
                    </button>
                  )}
                  {transcript && (
                    <button className="link-btn" onClick={clearTranscript}>
                      Clear
                    </button>
                  )}
                </span>
              </div>
              <textarea
                ref={transcriptRef}
                className="transcript-field"
                readOnly
                rows={6}
                value={transcript}
                placeholder={
                  sttEngine === "local"
                    ? "The conversation will appear here as it is transcribed (waiting for the model and for audio to play)..."
                    : "The conversation will appear here as it is transcribed (waiting for audio to play)..."
                }
              />
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
            <div className="assistant-panes">
              <div className="pane">
                <div className="pane-title">History</div>
                {qa.map((e) => (
                  <QaCard key={e.id} e={e} />
                ))}
              </div>
              <div className="pane">
                <div className="pane-title">Code &amp; Math</div>
                {qa.some((e) => !e.pending && !e.error && hasCodeOrMath(e.a)) ? (
                  qa
                    .filter((e) => !e.pending && !e.error && hasCodeOrMath(e.a))
                    .map((e) => <QaCard key={e.id} e={e} />)
                ) : (
                  <div className="muted sm">Code snippets and equations from answers show up here.</div>
                )}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
