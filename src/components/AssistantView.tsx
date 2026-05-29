import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Bot, Ear, KeyRound, Send, Square } from "lucide-react";
import * as api from "../api";
import { ASSISTANT_MODELS } from "../types";

export function AssistantView() {
  const [hasKey, setHasKey] = useState(false);
  const [hasSttKey, setHasSttKey] = useState(false);
  const [sttEngine, setSttEngine] = useState("deepgram");
  const [model, setModel] = useState("claude-opus-4-8");
  const [keyInput, setKeyInput] = useState("");
  const [sttKeyInput, setSttKeyInput] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [savingStt, setSavingStt] = useState(false);

  const [listening, setListening] = useState(false);
  const [listenStatus, setListenStatus] = useState("");
  const [transcript, setTranscript] = useState("");
  const [listenError, setListenError] = useState("");

  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const loadSettings = useCallback(async () => {
    const s = await api.getAssistantSettings();
    setHasKey(s.hasKey);
    setHasSttKey(s.hasSttKey);
    setSttEngine(s.sttEngine);
    setModel(s.model);
  }, []);
  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

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

  const ask = async () => {
    if (!question.trim() || loading) return;
    setLoading(true);
    setError("");
    setAnswer("");
    try {
      setAnswer(await api.askAssistant(question.trim(), context.trim() || null));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
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
            assistant sends your question and any context you paste to Anthropic to generate the answer.
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
              {listening && <span className="muted sm pulse">{listenStatus || "Capturing..."}</span>}
              {!listening && sttEngine === "local" && (
                <span className="muted sm">
                  Runs on your CPU, audio stays local. First run downloads a model (~290MB).
                </span>
              )}
              {transcript && (
                <button className="link-btn" onClick={() => setTranscript("")}>
                  Clear
                </button>
              )}
            </div>
          )}

          {listenError && <div className="assistant-error">{listenError}</div>}

          {transcript && (
            <div className="transcript">
              <div className="transcript-text">{transcript}</div>
              <button className="btn-open" onClick={() => setContext(transcript)}>
                Use as context
              </button>
            </div>
          )}

          <textarea
            className="assistant-context"
            placeholder="Optional context: paste a problem statement or notes, or use the live transcript above..."
            value={context}
            onChange={(e) => setContext(e.target.value)}
            rows={3}
          />
          <div className="assistant-ask">
            <textarea
              placeholder="What do you need? e.g. 'Solve 3x + 7 = 22' or 'Answer what they just asked'"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void ask();
              }}
              rows={2}
            />
            <button className="btn-sync" onClick={ask} disabled={loading || !question.trim()}>
              <Send size={16} /> {loading ? "Thinking..." : "Ask"}
            </button>
          </div>
          <div className="muted sm assistant-hint">Ctrl+Enter to ask.</div>

          {error && <div className="assistant-error">{error}</div>}
          {answer && <div className="assistant-answer">{answer}</div>}
        </>
      )}
    </div>
  );
}
