import { useCallback, useEffect, useState } from "react";
import { Bot, KeyRound, Send } from "lucide-react";
import * as api from "../api";
import { ASSISTANT_MODELS } from "../types";

export function AssistantView() {
  const [hasKey, setHasKey] = useState(false);
  const [model, setModel] = useState("claude-opus-4-8");
  const [keyInput, setKeyInput] = useState("");
  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [savingKey, setSavingKey] = useState(false);

  const loadSettings = useCallback(async () => {
    const s = await api.getAssistantSettings();
    setHasKey(s.hasKey);
    setModel(s.model);
  }, []);
  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

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

  const removeKey = async () => {
    await api.setApiKey("");
    await loadSettings();
  };

  const changeModel = async (m: string) => {
    setModel(m);
    await api.setAssistantModel(m);
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

  return (
    <div className="assistant">
      <div className="assistant-bar">
        <div className="assistant-title">
          <Bot size={18} /> Assistant
        </div>
        <select value={model} onChange={(e) => void changeModel(e.target.value)} title="Model">
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
          <textarea
            className="assistant-context"
            placeholder="Optional context: paste a transcript snippet, a problem statement, or notes..."
            value={context}
            onChange={(e) => setContext(e.target.value)}
            rows={3}
          />
          <div className="assistant-ask">
            <textarea
              placeholder="What do you need? e.g. 'Solve 3x + 7 = 22' or 'Explain what they just asked'"
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

          <div className="assistant-keyrow muted sm">
            <KeyRound size={13} /> API key connected.
            <button className="link-btn" onClick={removeKey}>
              Remove
            </button>
          </div>
        </>
      )}
    </div>
  );
}
