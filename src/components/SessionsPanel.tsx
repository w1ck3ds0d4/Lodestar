import { useCallback, useEffect, useState } from "react";
import { Sparkles, Trash2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import * as api from "../api";
import type { Session } from "../types";

// Saved interview sessions: review past transcripts/answers and run an AI coach pass
// that surfaces weak areas and what to study next.
export function SessionsPanel({ reload }: { reload: number }) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const load = useCallback(async () => {
    try {
      setSessions(await api.listSessions());
    } catch {
      /* ignore */
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load, reload]);

  if (sessions.length === 0) {
    return (
      <p className="muted sm">
        No saved sessions yet. After a Listen session or some answers, hit <strong>Save session</strong> in
        the dialog header - then review it here to see weak spots and what to study.
      </p>
    );
  }
  return (
    <div className="sessions">
      {sessions.map((s) => (
        <SessionCard key={s.id} session={s} onChanged={load} />
      ))}
    </div>
  );
}

function SessionCard({ session, onChanged }: { session: Session; onChanged: () => void }) {
  const [review, setReview] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  const runReview = async () => {
    setBusy(true);
    setErr("");
    try {
      setReview(await api.sessionReview(session.transcript, session.answers));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };
  const del = async () => {
    await api.removeSession(session.id);
    onChanged();
  };

  const when = new Date(session.createdAt).toLocaleString();
  return (
    <details className="session-card">
      <summary>
        <span className="session-title">{session.title}</span>
        <span className="muted sm">{when}</span>
      </summary>
      <div className="session-body">
        {session.transcript.trim() && (
          <details className="session-sub">
            <summary>Transcript</summary>
            <pre className="session-pre">{session.transcript}</pre>
          </details>
        )}
        {session.answers.trim() && (
          <details className="session-sub">
            <summary>Answers</summary>
            <pre className="session-pre">{session.answers}</pre>
          </details>
        )}
        <div className="session-actions">
          <button className="btn-sync" onClick={() => void runReview()} disabled={busy}>
            <Sparkles size={15} /> {busy ? "Reviewing..." : "Review & coach"}
          </button>
          <button className="icon-btn danger sm" title="Delete session" onClick={() => void del()}>
            <Trash2 size={14} />
          </button>
        </div>
        {err && <div className="assistant-error">{err}</div>}
        {review && (
          <div className="assistant-answer">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{review}</ReactMarkdown>
          </div>
        )}
      </div>
    </details>
  );
}
