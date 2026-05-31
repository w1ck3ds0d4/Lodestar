import { useState } from "react";
import { Building2, CalendarDays, Check, ExternalLink, MapPin, Plus, Wallet } from "lucide-react";
import { LEVEL_LABELS, REMOTE_REGION_LABELS, WORKPLACE_LABELS, type Job } from "../types";

const EXCERPT = 280;

export function JobCard({
  job,
  kindLabel,
  onOpen,
  onTrack,
}: {
  job: Job;
  kindLabel: string;
  onOpen: (url: string) => void;
  onTrack: (job: Job) => Promise<void> | void;
}) {
  const [open, setOpen] = useState(false);
  const [tracked, setTracked] = useState(false);
  const desc = job.description || "";
  const long = desc.length > EXCERPT;
  const shown = open || !long ? desc : `${desc.slice(0, EXCERPT)}…`;

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
