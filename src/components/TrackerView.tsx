import { useCallback, useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
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

export function TrackerView({ onOpen }: { onOpen: (url: string) => void }) {
  const [items, setItems] = useState<Tracked[]>([]);
  const load = useCallback(async () => setItems(await api.listTracked()), []);
  useEffect(() => {
    void load();
  }, [load]);

  const applications = items.filter((i) => i.kind === "application");
  const programs = items.filter((i) => i.kind === "program");

  return (
    <div className="tracker">
      <section>
        <h2 className="section-title">Applications</h2>
        {applications.length === 0 ? (
          <p className="muted">
            No applications yet. Hit <strong>Track</strong> on a job in Search to add one here.
          </p>
        ) : (
          <div className="board">
            {APP_STATUSES.map((st) => {
              const col = applications.filter((a) => a.status === st);
              return (
                <div className="board-col" key={st}>
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

  const move = async (status: string) => {
    await api.updateTracked(item.id, status, notes, item.progress, item.deadline);
    onChanged();
  };
  const saveNotes = async () => {
    if (notes !== item.notes) await api.updateTracked(item.id, item.status, notes, item.progress, item.deadline);
  };
  const del = async () => {
    await api.removeTracked(item.id);
    onChanged();
  };

  return (
    <div className="track-card">
      <div className="track-card-head">
        <button className="link-title" onClick={() => item.url && onOpen(item.url)}>{item.title}</button>
        <button className="icon-btn danger sm" title="Remove" onClick={del}><Trash2 size={14} /></button>
      </div>
      {item.org && <div className="muted sm">{item.org}</div>}
      <select value={item.status} onChange={(e) => move(e.target.value)}>
        {APP_STATUSES.map((s) => (
          <option key={s} value={s}>{APP_STATUS_LABELS[s]}</option>
        ))}
      </select>
      <textarea
        className="track-notes"
        placeholder="Notes (recruiter, salary, next step)..."
        value={notes}
        onChange={(e) => setNotes(e.target.value)}
        onBlur={saveNotes}
      />
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
