import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Bell, ClipboardList, GraduationCap, Radar, RefreshCw, Search, Settings } from "lucide-react";
import * as api from "./api";
import { KIND_LABELS, LEVELS, WORKPLACES, type Alert, type Job, type Source, type Stats } from "./types";
import { JobCard } from "./components/JobCard";
import { SourcesPanel } from "./components/SourcesPanel";
import { AlertsPanel } from "./components/AlertsPanel";
import { TrackerView } from "./components/TrackerView";
import { StudyView } from "./components/StudyView";

type View = "search" | "tracker" | "study";

export default function App() {
  const [view, setView] = useState<View>("search");
  const [query, setQuery] = useState("");
  const [workplaces, setWorkplaces] = useState<string[]>([]);
  const [levels, setLevels] = useState<string[]>([]);
  const [selectedKinds, setSelectedKinds] = useState<string[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [stats, setStats] = useState<Stats>({ totalJobs: 0, companies: 0 });
  const [sources, setSources] = useState<Source[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [panel, setPanel] = useState<"sources" | "alerts" | null>(null);

  const refreshSources = useCallback(async () => setSources(await api.listSources()), []);
  const refreshAlerts = useCallback(async () => setAlerts(await api.listAlerts()), []);
  const refreshStats = useCallback(async () => setStats(await api.getStats()), []);

  const runSearch = useCallback(async () => {
    setLoading(true);
    try {
      setJobs(await api.searchJobs(query, workplaces, levels, selectedKinds));
    } finally {
      setLoading(false);
    }
  }, [query, workplaces, levels, selectedKinds]);

  useEffect(() => {
    void refreshSources();
    void refreshAlerts();
    void refreshStats();
  }, [refreshSources, refreshAlerts, refreshStats]);

  // Debounced search whenever the query or filters change.
  useEffect(() => {
    const t = setTimeout(() => void runSearch(), 200);
    return () => clearTimeout(t);
  }, [runSearch]);

  // The background scheduler updates the DB; keep stats fresh.
  useEffect(() => {
    const t = setInterval(() => void refreshStats(), 15000);
    return () => clearInterval(t);
  }, [refreshStats]);

  const onSync = useCallback(async () => {
    setSyncing(true);
    try {
      await api.syncNow();
    } catch {
      setSyncing(false);
      return;
    }
    window.setTimeout(async () => {
      await Promise.all([refreshStats(), refreshSources()]);
      await runSearch();
      setSyncing(false);
    }, 5000);
  }, [refreshStats, refreshSources, runSearch]);

  const onOpen = useCallback((url: string) => void openUrl(url), []);

  const onTrack = useCallback(async (job: Job) => {
    await api.addTracked({
      kind: "application",
      title: job.title,
      org: job.company,
      url: job.url,
      status: "saved",
      notes: "",
      deadline: null,
      sourceJobId: job.id,
    });
  }, []);

  const availableKinds = useMemo(() => Array.from(new Set(sources.map((s) => s.kind))), [sources]);

  const toggle = (setter: Dispatch<SetStateAction<string[]>>, k: string) =>
    setter((prev) => (prev.includes(k) ? prev.filter((x) => x !== k) : [...prev, k]));

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <Radar size={22} className="brand-icon" />
          <span>JobRadar</span>
        </div>
        <nav className="nav">
          <button className={"nav-btn" + (view === "search" ? " active" : "")} onClick={() => setView("search")}>
            <Search size={15} /> Search
          </button>
          <button className={"nav-btn" + (view === "tracker" ? " active" : "")} onClick={() => setView("tracker")}>
            <ClipboardList size={15} /> Tracker
          </button>
          <button className={"nav-btn" + (view === "study" ? " active" : "")} onClick={() => setView("study")}>
            <GraduationCap size={15} /> Study
          </button>
        </nav>
        <div className="spacer" />
        <div className="stat-pill">
          {stats.totalJobs.toLocaleString()} jobs · {stats.companies.toLocaleString()} companies
        </div>
        <button className="icon-btn" title="Alerts" onClick={() => setPanel("alerts")}><Bell size={18} /></button>
        <button className="icon-btn" title="Sources" onClick={() => setPanel("sources")}><Settings size={18} /></button>
        <button className="btn-sync" onClick={onSync} disabled={syncing}>
          <RefreshCw size={16} className={syncing ? "spin" : ""} /> {syncing ? "Syncing…" : "Sync now"}
        </button>
      </header>

      {view === "search" ? (
        <>
          <div className="controls">
            <div className="search-box">
              <Search size={16} className="search-icon" />
              <input
                autoFocus
                placeholder="Search title, company, location, description…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
          </div>

          <div className="filters">
            <div className="filter-group">
              <span className="filter-label">Workplace</span>
              {WORKPLACES.map((w) => (
                <button
                  key={w.key}
                  className={"chip" + (workplaces.includes(w.key) ? " active" : "")}
                  onClick={() => toggle(setWorkplaces, w.key)}
                >
                  {w.label}
                </button>
              ))}
            </div>
            <div className="filter-group">
              <span className="filter-label">Level</span>
              {LEVELS.map((l) => (
                <button
                  key={l.key}
                  className={"chip" + (levels.includes(l.key) ? " active" : "")}
                  onClick={() => toggle(setLevels, l.key)}
                >
                  {l.label}
                </button>
              ))}
            </div>
          </div>

          {availableKinds.length > 0 && (
            <div className="chips">
              <span className="filter-label">Source</span>
              {availableKinds.map((k) => (
                <button
                  key={k}
                  className={"chip" + (selectedKinds.includes(k) ? " active" : "")}
                  onClick={() => toggle(setSelectedKinds, k)}
                >
                  {KIND_LABELS[k] ?? k}
                </button>
              ))}
              {(selectedKinds.length > 0 || workplaces.length > 0 || levels.length > 0) && (
                <button
                  className="chip clear"
                  onClick={() => {
                    setSelectedKinds([]);
                    setWorkplaces([]);
                    setLevels([]);
                  }}
                >
                  Clear all
                </button>
              )}
            </div>
          )}

          <main className="results">
            {loading && jobs.length === 0 ? (
              <div className="empty">Searching…</div>
            ) : jobs.length === 0 ? (
              <div className="empty">
                No jobs yet. Hit <strong>Sync now</strong> to pull from your sources, or add sources under{" "}
                <strong>Settings</strong>.
              </div>
            ) : (
              <>
                <div className="result-count">{jobs.length} result{jobs.length === 1 ? "" : "s"}</div>
                {jobs.map((j) => (
                  <JobCard
                    key={j.id}
                    job={j}
                    kindLabel={KIND_LABELS[j.sourceKind] ?? j.sourceKind}
                    onOpen={onOpen}
                    onTrack={onTrack}
                  />
                ))}
              </>
            )}
          </main>
        </>
      ) : (
        <main className="results">{view === "tracker" ? <TrackerView onOpen={onOpen} /> : <StudyView />}</main>
      )}

      {panel === "sources" && (
        <SourcesPanel
          sources={sources}
          onChanged={() => {
            void refreshSources();
            void refreshStats();
            void runSearch();
          }}
          onClose={() => setPanel(null)}
        />
      )}
      {panel === "alerts" && (
        <AlertsPanel
          alerts={alerts}
          presetQuery={query}
          onChanged={() => void refreshAlerts()}
          onClose={() => setPanel(null)}
        />
      )}
    </div>
  );
}
