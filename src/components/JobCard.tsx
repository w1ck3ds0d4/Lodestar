import { useMemo, useState } from "react";
import {
  AlertTriangle,
  Building2,
  CalendarDays,
  Check,
  ExternalLink,
  Gauge,
  MapPin,
  Plus,
  Wallet,
} from "lucide-react";
import {
  LEVEL_LABELS,
  REMOTE_REGION_LABELS,
  SCAM_FLAG_LABELS,
  SCAM_LIKELY,
  SCAM_REVIEW,
  WORKPLACE_LABELS,
  type Job,
} from "../types";

const EXCERPT = 280;

// Some sources (Greenhouse, Lever, career pages, ...) return job descriptions as
// HTML. Render them as readable plain text: turn block boundaries into spaces so
// words don't run together, then let the browser strip the remaining tags and
// decode entities (&amp;, &#39;, ...).
export function htmlToText(html: string): string {
  if (!html) return "";
  if (!html.includes("<")) return html;
  const withSpaces = html.replace(/<\/(p|div|li|h[1-6]|tr|ul|ol|table|section)>|<br\s*\/?>/gi, " ");
  const doc = new DOMParser().parseFromString(withSpaces, "text/html");
  return (doc.body?.textContent || "").replace(/\s+/g, " ").trim();
}

export function JobCard({
  job,
  kindLabel,
  onOpen,
  onTrack,
  onScore,
}: {
  job: Job;
  kindLabel: string;
  onOpen: (url: string) => void;
  onTrack: (job: Job) => Promise<void> | void;
  onScore: (job: Job) => void;
}) {
  const [open, setOpen] = useState(false);
  const [tracked, setTracked] = useState(false);
  const desc = useMemo(() => htmlToText(job.description || ""), [job.description]);
  const long = desc.length > EXCERPT;
  const shown = open || !long ? desc : `${desc.slice(0, EXCERPT)}…`;
  // Advisory scam badge: "Likely scam" (red, harm-gated) >=75, else "Review" >=40.
  const scamTier = job.scamScore >= SCAM_LIKELY ? "likely" : job.scamScore >= SCAM_REVIEW ? "review" : null;
  const scamWhy =
    job.scamFlags.length > 0
      ? `Why: ${job.scamFlags.map((f) => SCAM_FLAG_LABELS[f] ?? f).join("; ")}`
      : "Flagged for review";

  return (
    <article className="card">
      <div className="card-head">
        <h3 className="card-title">
          <button className="link-title" onClick={() => job.url && onOpen(job.url)}>{job.title}</button>
          {job.workplace && (
            <span className={`badge badge-${job.workplace}`}>
              {WORKPLACE_LABELS[job.workplace] ?? job.workplace}
            </span>
          )}
          {(job.experienceLevel === "entry" || job.experienceLevel === "senior") && (
            <span className={`badge badge-${job.experienceLevel}`}>{LEVEL_LABELS[job.experienceLevel]}</span>
          )}
          {REMOTE_REGION_LABELS[job.remoteRegion] && (
            <span className="badge badge-region">{REMOTE_REGION_LABELS[job.remoteRegion]}</span>
          )}
          {job.relocation && <span className="badge badge-reloc">Relocation</span>}
        </h3>
        <span className="badge badge-source">{kindLabel}</span>
        {job.alsoOn.length > 0 && (
          <span className="badge badge-also" title={`Also posted on: ${job.alsoOn.join(", ")}`}>
            +{job.alsoOn.length} {job.alsoOn.length === 1 ? "board" : "boards"}
          </span>
        )}
        {scamTier && (
          <span className={`badge badge-scam badge-scam-${scamTier}`} title={scamWhy}>
            <AlertTriangle size={12} /> {scamTier === "likely" ? "Likely scam" : "Review"}
          </span>
        )}
      </div>

      <div className="card-meta">
        {job.company && <span><Building2 size={14} /> {job.company}</span>}
        {job.location && <span><MapPin size={14} /> {job.location}</span>}
        {job.postedAt && <span><CalendarDays size={14} /> {job.postedAt}</span>}
        {job.salary && <span><Wallet size={14} /> {job.salary}</span>}
        {job.employmentType && <span className="muted">{job.employmentType}</span>}
      </div>

      {desc && <p className="card-desc">{shown}</p>}

      <div className="card-actions">
        {long && (
          <button className="btn-ghost" onClick={() => setOpen((o) => !o)}>
            {open ? "Show less" : "Show more"}
          </button>
        )}
        <div className="spacer" />
        <button
          className="btn-ghost"
          disabled={!desc}
          title={desc ? "Score this job against your saved resume" : "No description to score against"}
          onClick={() => onScore(job)}
        >
          <Gauge size={14} /> Score
        </button>
        <button
          className="btn-ghost"
          disabled={tracked}
          onClick={async () => {
            await onTrack(job);
            setTracked(true);
          }}
        >
          {tracked ? <><Check size={14} /> Tracking</> : <><Plus size={14} /> Track</>}
        </button>
        {job.url && (
          <button className="btn-open" onClick={() => onOpen(job.url)}>
            <ExternalLink size={14} /> Open
          </button>
        )}
      </div>
    </article>
  );
}
