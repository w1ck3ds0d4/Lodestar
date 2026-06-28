import { useCallback, useEffect, useRef, useState, type ChangeEvent } from "react";
import { Plus, Trash2, Upload, UserRound, X } from "lucide-react";
import * as api from "../api";
import type { Profile } from "../types";
import { StoriesPanel } from "./StoriesPanel";
import { extractResumeText } from "../lib/extractText";

// The dedicated "your profiles" hub, opened from the person icon in the header. Each
// profile is a persona (role, company, resume, notes, plus its own stories); the active
// one drives the live assistant answers, the screen/hybrid answer, and the phone remote.
// Rendered as a slide-over that becomes full-width on a narrow screen.
export function ProfileHub({
  open,
  onClose,
  onActiveChange,
}: {
  open: boolean;
  onClose: () => void;
  onActiveChange?: (id: number) => void;
}) {
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [activeId, setActiveId] = useState<number | null>(null);
  // Editor form, populated from the active profile.
  const [name, setName] = useState("");
  const [role, setRole] = useState("");
  const [company, setCompany] = useState("");
  const [resume, setResume] = useState("");
  const [notes, setNotes] = useState("");
  const [saved, setSaved] = useState(false);
  const [resumeMsg, setResumeMsg] = useState("");
  const [err, setErr] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  const fillForm = useCallback((p: Profile | undefined) => {
    setName(p?.name ?? "");
    setRole(p?.role ?? "");
    setCompany(p?.company ?? "");
    setResume(p?.resume ?? "");
    setNotes(p?.notes ?? "");
    setResumeMsg("");
  }, []);

  const load = useCallback(async () => {
    try {
      const [list, active] = await Promise.all([api.listProfiles(), api.activeProfile()]);
      setProfiles(list);
      setActiveId(active);
      fillForm(list.find((p) => p.id === active) ?? list[0]);
    } catch (e) {
      setErr(String(e));
    }
  }, [fillForm]);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const select = async (id: number) => {
    if (id === activeId) return;
    setErr("");
    try {
      await api.setActiveProfile(id);
      setActiveId(id);
      fillForm(profiles.find((p) => p.id === id));
      onActiveChange?.(id);
    } catch (e) {
      setErr(String(e));
    }
  };

  const addProfile = async () => {
    setErr("");
    try {
      const p = await api.createProfile("New profile");
      await api.setActiveProfile(p.id);
      setProfiles((prev) => [...prev, p]);
      setActiveId(p.id);
      fillForm(p);
      onActiveChange?.(p.id);
    } catch (e) {
      setErr(String(e));
    }
  };

  const save = async () => {
    if (activeId == null) return;
    setErr("");
    try {
      await api.updateProfile(activeId, name, role, company, resume, notes);
      setProfiles((prev) =>
        prev.map((p) => (p.id === activeId ? { ...p, name, role, company, resume, notes } : p)),
      );
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
      onActiveChange?.(activeId);
    } catch (e) {
      setErr(String(e));
    }
  };

  const del = async () => {
    if (activeId == null || profiles.length <= 1) {
      setErr("Keep at least one profile.");
      return;
    }
    if (!window.confirm(`Delete profile "${name || "Untitled"}" and its stories?`)) return;
    setErr("");
    try {
      await api.deleteProfile(activeId);
      await load();
      const next = await api.activeProfile().catch(() => null);
      if (next != null) onActiveChange?.(next);
    } catch (e) {
      setErr(String(e));
    }
  };

  const onResumeFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = ""; // allow re-picking the same file later
    if (!file) return;
    setResumeMsg("Reading...");
    try {
      const text = await extractResumeText(file);
      if (!text.trim()) {
        setResumeMsg("No selectable text found in that file (a scanned PDF has none).");
        return;
      }
      setResume(text);
      setResumeMsg(`Imported ${text.length.toLocaleString()} chars from ${file.name} - review and Save.`);
    } catch (e2) {
      setResumeMsg(String(e2));
    }
  };

  if (!open) return null;

  return (
    <div className="hub-overlay" onClick={onClose}>
      <aside className="hub-panel" onClick={(e) => e.stopPropagation()}>
        <header className="hub-head">
          <div className="hub-title">
            <UserRound size={18} /> Your profiles
          </div>
          <button className="icon-btn" title="Close" onClick={onClose}>
            <X size={18} />
          </button>
        </header>
        <p className="muted sm">
          Each profile is a persona (role, company, resume, notes, and its own stories). The active one
          tailors the live assistant answers, the screen answer, and the phone remote.
        </p>

        <div className="hub-profiles">
          {profiles.map((p) => (
            <button
              key={p.id}
              className={"hub-chip" + (p.id === activeId ? " active" : "")}
              onClick={() => void select(p.id)}
            >
              {p.name || "Untitled"}
            </button>
          ))}
          <button className="hub-chip add" onClick={() => void addProfile()}>
            <Plus size={14} /> New
          </button>
        </div>

        {err && <div className="assistant-error">{err}</div>}

        <div className="hub-editor">
          <div className="setup-field">
            <span className="setup-label">Profile name</span>
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Senior Backend" />
          </div>
          <div className="setup-field">
            <span className="setup-label">Target role / focus</span>
            <input
              value={role}
              onChange={(e) => setRole(e.target.value)}
              placeholder="e.g. Senior Backend Engineer"
            />
          </div>
          <div className="setup-field">
            <span className="setup-label">Company (for prep / briefs)</span>
            <input
              value={company}
              onChange={(e) => setCompany(e.target.value)}
              placeholder="e.g. Acme Corp"
            />
          </div>
          <div className="setup-field">
            <span className="setup-label">Resume / background</span>
            <textarea
              rows={5}
              value={resume}
              onChange={(e) => setResume(e.target.value)}
              placeholder="Paste your resume, or import a file…"
            />
            <div className="resume-import">
              <input
                ref={fileRef}
                type="file"
                accept=".pdf,.docx,.txt,.md"
                className="hidden-file"
                onChange={(e) => void onResumeFile(e)}
              />
              <button className="btn-ghost" onClick={() => fileRef.current?.click()}>
                <Upload size={14} /> Import file
              </button>
              {resumeMsg && <span className="muted sm">{resumeMsg}</span>}
            </div>
          </div>
          <div className="setup-field">
            <span className="setup-label">Notes / materials</span>
            <textarea
              rows={3}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              placeholder="Anything else the assistant should know…"
            />
          </div>
          <div className="hub-actions">
            <button className="btn-open" onClick={() => void save()}>
              {saved ? "Saved ✓" : "Save profile"}
            </button>
            <button
              className="icon-btn danger"
              title="Delete profile"
              onClick={() => void del()}
              disabled={profiles.length <= 1}
            >
              <Trash2 size={15} />
            </button>
          </div>
        </div>

        <div className="hub-stories">
          <div className="setup-label">Experience stories (this profile)</div>
          {activeId != null && <StoriesPanel key={activeId} />}
        </div>
      </aside>
    </div>
  );
}
