import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Bot, Ear, KeyRound, Send, Square } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import * as api from "../api";
import { ASSISTANT_MODELS } from "../types";

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

// A sentence looks like a question/request if it ends with "?" or opens with an
// interrogative or an imperative ("solve", "calculate", "explain", ...).
const QUESTION_START =
  /^(what'?s?|how|why|when|where|who|whose|which|can you|could you|would you|do you|does|is|are|solve|calculate|compute|evaluate|simplify|factor|find|derive|prove|explain|define|describe|convert|translate|write|implement|debug|fix|show|give|list|summari[sz]e)\b/i;
function isQuestionLike(s: string): boolean {
  const t = s.trim();
  return t.endsWith("?") || t.includes("¿") || QUESTION_START.test(t);
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

  const loadSettings = useCallback(async () => {
    const s = await api.getAssistantSettings();
    setHasKey(s.hasKey);
    setHasSttKey(s.hasSttKey);
    setSttEngine(s.sttEngine);
    setCaptureSource(s.captureSource);
    setModel(s.model);
  }, []);
  useEffect(() => {
    void loadSettings();
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

  // Live transcript + listen lifecycle events from the Rust capture loop.
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    void listen<{ text: string }>("transcript", (e) => {
      setTranscript((t) => (t ? t + " " : "") + e.payload.text);
    }).then((u) => unlisteners.push(u));
    void listen<string>("listen-status", (e) => setListenStatus(String(e.payload))).then((u) =>
      unlisteners.push(u),
    );
    void listen<string>("listen-error", (e) => {
      setListenError(String(e.payload));
      setListening(false);
      setListenStatus("");
    }).then((u) => unlisteners.push(u));
    void listen("listen-stopped", () => {
      setListening(false);
      setListenStatus("");
    }).then((u) => unlisteners.push(u));
    return () => unlisteners.forEach((u) => u());
  }, []);

  // Auto-answer: scan newly-completed sentences and answer ones that look like a
  // question or a request (ends with "?", or opens with what/how/solve/calculate/...).
  useEffect(() => {
    if (!autoAnswer || !listening) return;
    const pending = transcript.slice(answeredIdx.current);
    const sentenceRe = /[^.!?]*[.!?]/g;
    let m: RegExpExecArray | null;
    let consumed = 0;
    while ((m = sentenceRe.exec(pending)) !== null) {
      const sentence = m[0].trim();
      consumed = m.index + m[0].length;
      if (sentence.length >= 4 && isQuestionLike(sentence)) {
        void ask(sentence, transcript.slice(0, answeredIdx.current + consumed));
      }
    }
    answeredIdx.current += consumed;
  }, [transcript, autoAnswer, listening, ask]);

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
              <div className="transcript-text">
                {transcript || (
                  <span className="muted">
                    {sttEngine === "local"
                      ? "Waiting for the model and for system audio to play..."
                      : "Waiting for system audio to play..."}
                  </span>
                )}
              </div>
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
