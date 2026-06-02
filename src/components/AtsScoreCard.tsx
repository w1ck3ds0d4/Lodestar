import type { AtsScore } from "../types";

// The five model-provided sub-scores, in display order.
const SUBS: { key: "keywords" | "responsibilities" | "title" | "impact" | "format"; label: string }[] = [
  { key: "keywords", label: "Keywords" },
  { key: "responsibilities", label: "Responsibilities" },
  { key: "title", label: "Title fit" },
  { key: "impact", label: "Impact" },
  { key: "format", label: "Format" },
];

// Renders a structured ATS match score: overall number, per-dimension bars, matched
// (green) / missing (red) keyword chips, and suggested fixes. Reused by the
// Application Kit and the one-click "score against my resume" on a job card.
export function AtsScoreCard({ score }: { score: AtsScore }) {
  return (
    <div className="ats-card">
      <div className="ats-overall">
        <span className="ats-overall-num">{score.overall}</span>
        <span className="ats-overall-lbl">ATS match</span>
      </div>
      <div className="ats-bars">
        {SUBS.map((s) => (
          <div key={s.key} className="ats-bar-row">
            <span className="ats-bar-lbl">{s.label}</span>
            <div className="ats-bar">
              <div className="ats-bar-fill" style={{ width: `${score[s.key]}%` }} />
            </div>
            <span className="ats-bar-num">{score[s.key]}</span>
          </div>
        ))}
      </div>
      {score.matched.length > 0 && (
        <div className="ats-tags">
          <span className="ats-tags-lbl">Matched</span>
          {score.matched.map((m) => (
            <span key={m} className="ats-chip ok">
              {m}
            </span>
          ))}
        </div>
      )}
      {score.missing.length > 0 && (
        <div className="ats-tags">
          <span className="ats-tags-lbl">Missing</span>
          {score.missing.map((m) => (
            <span key={m} className="ats-chip miss">
              {m}
            </span>
          ))}
        </div>
      )}
      {score.fixes.length > 0 && (
        <div className="ats-fixes">
          <span className="ats-tags-lbl">Suggested fixes</span>
          <ul>
            {score.fixes.map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
