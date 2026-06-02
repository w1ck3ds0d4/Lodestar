import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import * as api from "../api";
import type { AtsScore } from "../types";
import { AtsScoreCard } from "./AtsScoreCard";

// Resume + JD engine: paste a job description and tailor your materials to it, grounded
// in the stored resume/profile. "ATS score" returns a structured scorecard; the thank-you
// note uses the latest interview transcript.
export function ApplicationKit({ transcript }: { transcript: string }) {
  const [jd, setJd] = useState("");
  const [out, setOut] = useState("");
  const [score, setScore] = useState<AtsScore | null>(null);
  const [busy, setBusy] = useState("");
  const [err, setErr] = useState("");
  const [copied, setCopied] = useState(false);

  const run = async (kind: string, input: string) => {
    if (!input.trim()) {
      setErr(
        kind === "thanks"
          ? "No interview transcript yet - run a Listen session, or paste notes in the box above."
          : "Paste the job description first.",
      );
      return;
    }
    setBusy(kind);
    setErr("");
    setOut("");
    setScore(null);
    try {
      setOut(await api.applicationDoc(kind, input));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy("");
    }
  };

  const runScore = async () => {
    if (!jd.trim()) {
      setErr("Paste the job description first.");
      return;
    }
    setBusy("score");
    setErr("");
    setOut("");
    setScore(null);
    try {
      setScore(await api.scoreAts(jd));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="appkit">
      <p className="muted sm">
        Paste a job description, then tailor your materials to it - grounded in your saved resume, profile,
        and stories.
      </p>
      <textarea
        rows={4}
        placeholder="Paste the job description here..."
        value={jd}
        onChange={(e) => setJd(e.target.value)}
      />
      <div className="appkit-actions">
        <button className="btn-sync" onClick={() => void runScore()} disabled={!!busy}>
          {busy === "score" ? "Scoring..." : "ATS score"}
        </button>
        <button className="btn-sync" onClick={() => void run("bullets", jd)} disabled={!!busy}>
          {busy === "bullets" ? "Tailoring..." : "Tailor bullets"}
        </button>
        <button className="btn-sync" onClick={() => void run("cover", jd)} disabled={!!busy}>
          {busy === "cover" ? "Writing..." : "Cover letter"}
        </button>
        <button
          className="btn-sync"
          onClick={() => void run("thanks", transcript)}
          disabled={!!busy}
          title="Thank-you / follow-up email from your latest interview transcript"
        >
          {busy === "thanks" ? "Writing..." : "Thank-you note"}
        </button>
      </div>
      {err && <div className="assistant-error">{err}</div>}

      {score && <AtsScoreCard score={score} />}

      {out && (
        <div className="appkit-out">
          <button
            className="link-btn"
            onClick={() => {
              void navigator.clipboard?.writeText(out).then(
                () => {
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                },
                () => {},
              );
            }}
          >
            {copied ? "Copied ✓" : "Copy"}
          </button>
          <div className="assistant-answer">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{out}</ReactMarkdown>
          </div>
        </div>
      )}
    </div>
  );
}
