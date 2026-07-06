import { useCallback, useEffect, useState } from "react";
import { Download, FolderOpen, Sparkles, Trash2 } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import * as api from "../api";
import type { Session } from "../types";

// Saved interview sessions: review past transcripts/answers, run an AI coach pass that
// surfaces weak areas and what to study next, and export them to Markdown files (they
// otherwise live only in the local database).
export function SessionsPanel({ reload }: { reload: number }) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [exportMsg, setExportMsg] = useState("");
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

  const exportAll = async () => {
    setExportMsg("Exporting...");
    try {
      const { dir, count } = await api.exportAllSessions();
      setExportMsg(`Exported ${count} session${count === 1 ? "" : "s"} to ${dir}`);
      void revealItemInDir(dir).catch(() => {});
    } catch (e) {
      setExportMsg(String(e));
    }
  };

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
      <div className="sessions-toolbar">
        <button
          className="link-btn"
          onClick={() => void exportAll()}
          title="Save every session as a Markdown file in Downloads/Lodestar"
        >
          <Download size={14} /> Export all to files
        </button>
      </div>
      {exportMsg && <div className="muted sm session-exported">{exportMsg}</div>}
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
  const [exported, setExported] = useState("");

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
  const exportOne = async () => {
    setErr("");
    try {
      const path = await api.exportSession(session.id);
      setExported(path);
      void revealItemInDir(path).catch(() => {});
    } catch (e) {
      setErr(String(e));
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
          <button
            className="btn-ghost sm"
            title="Export this session to a Markdown file"
            onClick={() => void exportOne()}
          >
            <Download size={14} /> Export
          </button>
          <button className="icon-btn danger sm" title="Delete session" onClick={() => void del()}>
            <Trash2 size={14} />
          </button>
        </div>
        {exported && (
          <div className="muted sm session-exported">
            Saved to <code>{exported}</code>{" "}
            <button className="link-btn" onClick={() => void revealItemInDir(exported).catch(() => {})}>
              <FolderOpen size={12} /> Show
            </button>
          </div>
        )}
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
