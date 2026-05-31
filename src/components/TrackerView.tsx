import { useCallback, useEffect, useState, type DragEvent } from "react";
import { GripVertical, Plus, Sparkles, Trash2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import * as api from "../api";
import {
  APP_STATUSES,
  APP_STATUS_LABELS,
  PROGRAM_CATALOG,
  PROGRAM_STATUSES,
  PROGRAM_STATUS_LABELS,
  type CatalogProgram,
  type Tracked,
} from "../types";

const RESPONDED = ["screen", "interview", "offer", "accepted"];
const INTERVIEWING = ["interview", "offer", "accepted"];

function Md({ text }: { text: string }) {
  return (
    <div className="assistant-answer">
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

// Classify a deadline relative to today (for highlighting), ignoring closed items.
function deadlineState(deadline: string | null, status: string): "" | "overdue" | "soon" {
  if (!deadline || status === "accepted" || status === "rejected") return "";
  const today = new Date().toISOString().slice(0, 10);
  const soon = new Date(Date.now() + 7 * 86400000).toISOString().slice(0, 10);
  if (deadline < today) return "overdue";
  if (deadline <= soon) return "soon";
  return "";
}

export function TrackerView({ onOpen }: { onOpen: (url: string) => void }) {
  const [items, setItems] = useState<Tracked[]>([]);
  const load = useCallback(async () => setItems(await api.listTracked()), []);
  useEffect(() => {
    void load();
  }, [load]);

  const applications = items.filter((i) => i.kind === "application");
  const programs = items.filter((i) => i.kind === "program");

  // Pipeline stats.
  const active = applications.filter((a) => a.status !== "accepted" && a.status !== "rejected").length;
  const appliedPlus = applications.filter((a) => a.status !== "saved").length;
  const responded = applications.filter((a) => RESPONDED.includes(a.status)).length;
  const interviews = applications.filter((a) => INTERVIEWING.includes(a.status)).length;
  const offers = applications.filter((a) => a.status === "offer" || a.status === "accepted").length;
  const responseRate = appliedPlus ? Math.round((responded / appliedPlus) * 100) : 0;
  const overdue = applications.filter((a) => deadlineState(a.deadline, a.status) === "overdue").length;
  const soon = applications.filter((a) => deadlineState(a.deadline, a.status) === "soon").length;

  // Drag a card onto a column to change its status.
  const onDropTo = (status: string) => async (e: DragEvent) => {
    e.preventDefault();
    const id = Number(e.dataTransfer.getData("text/plain"));
    const item = applications.find((a) => a.id === id);
    if (!item || item.status === status) return;
    await api.updateTracked(id, status, item.notes, item.progress, item.deadline);
    load();
  };

  return (
    <div className="tracker">
      <section>
        <h2 className="section-title">Applications</h2>
        {applications.length > 0 && (
          <div className="pipeline-stats">
            <span className="stat-pill">{active} active</span>
            <span className="stat-pill">{responseRate}% response</span>
            <span className="stat-pill">{interviews} interviewing</span>
            <span className="stat-pill">{offers} offers</span>
            {overdue + soon > 0 && (
              <span className={"stat-pill due" + (overdue > 0 ? " overdue" : "")}>
                {overdue > 0 && `${overdue} overdue`}
                {overdue > 0 && soon > 0 && " · "}
                {soon > 0 && `${soon} due soon`}
              </span>
            )}
          </div>
        )}
        {applications.length === 0 ? (
          <p className="muted">
            No applications yet. Hit <strong>Track</strong> on a job in Search to add one here.
          </p>
        ) : (
          <div className="board">
            {APP_STATUSES.map((st) => {
              const col = applications.filter((a) => a.status === st);
              return (
                <div
                  className="board-col"
                  key={st}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={onDropTo(st)}
                >
                  <div className="board-col-head">
                    {APP_STATUS_LABELS[st]} <span className="count">{col.length}</span>
                  </div>
                  {col.map((a) => (
                    <AppCard key={a.id} item={a} onChanged={load} onOpen={onOpen} />
                  ))}
                </div>
              );
            })}
          </div>
        )}
      </section>

      <section>
        <h2 className="section-title">Training &amp; Platforms</h2>
        <AddProgram onChanged={load} />
        {programs.length === 0 ? (
          <p className="muted">Add a platform or program above to start tracking your progress in it.</p>
        ) : (
          <div className="prog-grid">
            {programs.map((p) => (
              <ProgramCard key={p.id} item={p} onChanged={load} onOpen={onOpen} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function AppCard({ item, onChanged, onOpen }: { item: Tracked; onChanged: () => void; onOpen: (url: string) => void }) {
  const [notes, setNotes] = useState(item.notes);
  const [nextAction, setNextAction] = useState(item.nextAction ?? "");
  const [prepping, setPrepping] = useState(false);
  const [prepError, setPrepError] = useState("");
  const dl = deadlineState(item.deadline, item.status);

  const move = async (status: string) => {
    await api.updateTracked(item.id, status, notes, item.progress, item.deadline);
    onChanged();
  };
  const setDeadline = async (deadline: string | null) => {
    await api.updateTracked(item.id, item.status, notes, item.progress, deadline);
    onChanged();
  };
  const saveNotes = async () => {
    if (notes !== item.notes) await api.updateTracked(item.id, item.status, notes, item.progress, item.deadline);
  };
  const saveNextAction = async () => {
    if (nextAction !== (item.nextAction ?? "")) await api.setTrackedNextAction(item.id, nextAction);
  };
  const del = async () => {
    await api.removeTracked(item.id);
    onChanged();
  };
  const prep = async () => {
    setPrepping(true);
    setPrepError("");
    try {
      await api.prepApplication(item.id, item.title, item.org);
      onChanged();
    } catch (e) {
      setPrepError(String(e));
    } finally {
      setPrepping(false);
    }
  };

  return (
    <div className="track-card">
      <div className="track-card-head">
        <span
          className="drag-grip"
          draggable
          onDragStart={(e) => e.dataTransfer.setData("text/plain", String(item.id))}
          title="Drag to another column"
        >
          <GripVertical size={14} />
        </span>
        <button className="link-title" onClick={() => item.url && onOpen(item.url)}>{item.title}</button>
        <button className="icon-btn danger sm" title="Remove" onClick={del}><Trash2 size={14} /></button>
      </div>
      {item.org && <div className="muted sm">{item.org}</div>}
      <div className="track-row">
        <select value={item.status} onChange={(e) => move(e.target.value)}>
          {APP_STATUSES.map((s) => (
            <option key={s} value={s}>{APP_STATUS_LABELS[s]}</option>
          ))}
        </select>
        <input
          type="date"
          className={"track-deadline" + (dl ? " " + dl : "")}
          value={item.deadline ?? ""}
          title="Deadline"
          onChange={(e) => setDeadline(e.target.value || null)}
        />
      </div>
      <input
        className="track-next"
        placeholder="Next action (e.g. follow up Fri)..."
        value={nextAction}
        onChange={(e) => setNextAction(e.target.value)}
        onBlur={saveNextAction}
      />
      <textarea
        className="track-notes"
        placeholder="Notes (recruiter, salary, links)..."
        value={notes}
        onChange={(e) => setNotes(e.target.value)}
        onBlur={saveNotes}
      />
      <button className="btn-sync sm" onClick={prep} disabled={prepping}>
        <Sparkles size={13} /> {prepping ? "Prepping…" : item.prep ? "Refresh prep" : "Interview prep"}
      </button>
      {prepError && <div className="assistant-error">{prepError}</div>}
      {item.prep && (
        <details className="track-prep">
          <summary className="muted sm">Interview prep</summary>
          <Md text={item.prep} />
        </details>
      )}
    </div>
  );
}

function ProgramCard({ item, onChanged, onOpen }: { item: Tracked; onChanged: () => void; onOpen: (url: string) => void }) {
  const [notes, setNotes] = useState(item.notes);
  const [newMs, setNewMs] = useState("");

  const patch = async (status: string, deadline: string | null, n: string) => {
    await api.updateTracked(item.id, status, n, item.progress, deadline);
    onChanged();
  };
  const addMs = async () => {
    if (!newMs.trim()) return;
    await api.addMilestone(item.id, newMs.trim());
    setNewMs("");
    onChanged();
  };
  const del = async () => {
    await api.removeTracked(item.id);
    onChanged();
  };

  return (
    <div className="prog-card">
      <div className="track-card-head">
        <button className="link-title" onClick={() => item.url && onOpen(item.url)}>{item.title}</button>
        <button className="icon-btn danger sm" title="Remove" onClick={del}><Trash2 size={14} /></button>
      </div>
      {item.org && <div className="muted sm">{item.org}</div>}
      <div className="prog-row">
        <select value={item.status} onChange={(e) => patch(e.target.value, item.deadline, notes)}>
          {PROGRAM_STATUSES.map((s) => (
            <option key={s} value={s}>{PROGRAM_STATUS_LABELS[s]}</option>
          ))}
        </select>
        <input
          type="date"
          value={item.deadline ?? ""}
          onChange={(e) => patch(item.status, e.target.value || null, notes)}
        />
      </div>
      <div className="progress-bar"><div className="progress-fill" style={{ width: `${item.progress}%` }} /></div>
      <div className="ms-list">
        {item.milestones.map((m) => (
          <label key={m.id} className="ms-item">
            <input
              type="checkbox"
              checked={m.done}
              onChange={async (e) => {
                await api.setMilestone(m.id, e.target.checked);
                onChanged();
              }}
            />
            <span className={m.done ? "done" : ""}>{m.label}</span>
            <button
              className="icon-btn danger xs"
              title="Remove milestone"
              onClick={async () => {
                await api.removeMilestone(m.id);
                onChanged();
              }}
            >
              <Trash2 size={12} />
            </button>
          </label>
        ))}
        <div className="ms-add">
          <input
            placeholder="Add a milestone..."
            value={newMs}
            onChange={(e) => setNewMs(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addMs();
            }}
          />
          <button className="icon-btn" title="Add milestone" onClick={addMs}><Plus size={14} /></button>
        </div>
      </div>
      <textarea
        className="track-notes"
        placeholder="Notes..."
        value={notes}
        onChange={(e) => setNotes(e.target.value)}
        onBlur={() => patch(item.status, item.deadline, notes)}
      />
    </div>
  );
}

function AddProgram({ onChanged }: { onChanged: () => void }) {
  const [custom, setCustom] = useState({ title: "", org: "", url: "" });

  const add = async (kind: { title: string; org: string; url: string }) => {
    await api.addTracked({
      kind: "program",
      title: kind.title,
      org: kind.org,
      url: kind.url,
      status: "interested",
      notes: "",
      deadline: null,
      sourceJobId: null,
    });
    onChanged();
  };
  const addCatalog = (p: CatalogProgram) => add({ title: p.name, org: p.org, url: p.url });
  const addCustom = async () => {
    if (!custom.title.trim()) return;
    await add({ title: custom.title.trim(), org: custom.org.trim(), url: custom.url.trim() });
    setCustom({ title: "", org: "", url: "" });
  };

  return (
    <div className="add-program">
      <div className="catalog">
        {PROGRAM_CATALOG.map((p) => (
          <button key={p.name} className="catalog-chip" title={`${p.org} - ${p.tag}`} onClick={() => addCatalog(p)}>
            <Plus size={12} /> {p.name}
          </button>
        ))}
      </div>
      <div className="add-row">
        <input placeholder="Custom program / platform" value={custom.title} onChange={(e) => setCustom({ ...custom, title: e.target.value })} />
        <input placeholder="Org (optional)" value={custom.org} onChange={(e) => setCustom({ ...custom, org: e.target.value })} />
        <input placeholder="URL (optional)" value={custom.url} onChange={(e) => setCustom({ ...custom, url: e.target.value })} />
        <button className="btn-open" onClick={addCustom}><Plus size={14} /> Add</button>
      </div>
    </div>
  );
}
