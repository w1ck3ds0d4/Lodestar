import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Bell,
  Bot,
  ChevronDown,
  ClipboardList,
  Eye,
  EyeOff,
  GraduationCap,
  HelpCircle,
  Radar,
  RefreshCw,
  Search,
  Settings,
  UserRound,
  X,
} from "lucide-react";
import * as api from "./api";
import {
  KIND_LABELS,
  LANG_LABELS,
  LEVELS,
  WORKPLACES,
  type Alert,
  type AtsScore,
  type Job,
  type JobPrefs,
  type Source,
  type Stats,
} from "./types";
import { JobCard, htmlToText } from "./components/JobCard";
import { AtsScoreCard } from "./components/AtsScoreCard";
import { SourcesPanel } from "./components/SourcesPanel";
import { AlertsPanel } from "./components/AlertsPanel";
import { TrackerView } from "./components/TrackerView";
import { StudyView } from "./components/StudyView";
import { AssistantView } from "./components/AssistantView";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { GettingStarted } from "./components/GettingStarted";
import { ProfileHub } from "./components/ProfileHub";
import { ResizeHandles, WindowControls } from "./components/WindowChrome";

// The main window is borderless (decorations off), so it draws its own title bar (the top
// bar) and window controls. In a plain browser (Vite preview) there is no Tauri window, so
// skip the chrome there.
const HAS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type View = "search" | "tracker" | "study" | "assistant";

export default function App() {
  const [view, setView] = useState<View>("search");
  const [query, setQuery] = useState("");
  const [workplaces, setWorkplaces] = useState<string[]>([]);
  const [levels, setLevels] = useState<string[]>([]);
  const [selectedKinds, setSelectedKinds] = useState<string[]>([]);
  const [remoteEligible, setRemoteEligible] = useState(false);
  const [relocationOnly, setRelocationOnly] = useState(false);
  // Freshness filter: null = any time, otherwise jobs posted within N days.
  const [postedWithin, setPostedWithin] = useState<number | null>(null);
  // Minimum salary (nominal, in the posting's currency); null = any.
  const [minSalary, setMinSalary] = useState<number | null>(null);
  // Languages to keep (ISO 639-1); empty = all. Unknown-language jobs are always kept.
  const [langs, setLangs] = useState<string[]>([]);
  // Hide postings flagged "likely scam" (advisory; the badge always shows regardless).
  const [hideScams, setHideScams] = useState(false);
  // Resume-match controls (no-ops until a resume is saved in the Assistant).
  const [sortMode, setSortMode] = useState<"default" | "match">("default");
  // Minimum resume-match % to keep; null = any.
  const [minMatch, setMinMatch] = useState<number | null>(null);
  // The filter facets collapse to a single "Filters" pill by default; remembered.
  const [filtersOpen, setFiltersOpen] = useState(() => localStorage.getItem("jr.filtersOpen") === "1");
  const [jobs, setJobs] = useState<Job[]>([]);
  const [stats, setStats] = useState<Stats>({ totalJobs: 0, companies: 0 });
  const [sources, setSources] = useState<Source[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [panel, setPanel] = useState<"sources" | "alerts" | null>(null);
  const [capHidden, setCapHidden] = useState(true);
  const [showIntro, setShowIntro] = useState(() => localStorage.getItem("jr.introSeen") !== "1");
  const dismissIntro = useCallback(() => {
    localStorage.setItem("jr.introSeen", "1");
    setShowIntro(false);
  }, []);
  const [workCountry, setWorkCountry] = useState("");
  const [editingCountry, setEditingCountry] = useState(false);
  // Active one-click ATS score modal (null = closed).
  const [scoring, setScoring] = useState<{
    title: string;
    score: AtsScore | null;
    loading: boolean;
    err: string;
  } | null>(null);

  // The launch splash (static markup in index.html) is dismissed once the first search
  // resolves, i.e. the UI is populated and interactive.
  const [booted, setBooted] = useState(false);
  const [profileOpen, setProfileOpen] = useState(false);
  // Per-profile filter sync guards: prefsReady gates saving until the first load completes;
  // skipNextSave swallows the save that a programmatic load (mount or switch) would trigger.
  const prefsReady = useRef(false);
  const skipNextSave = useRef(false);

  const refreshSources = useCallback(async () => setSources(await api.listSources()), []);
  const refreshAlerts = useCallback(async () => setAlerts(await api.listAlerts()), []);
  const refreshStats = useCallback(async () => setStats(await api.getStats()), []);

  const runSearch = useCallback(async () => {
    setLoading(true);
    try {
      setJobs(
        await api.searchJobs(
          query,
          workplaces,
          levels,
          selectedKinds,
          relocationOnly,
          remoteEligible,
          postedWithin,
          minSalary,
          langs,
          hideScams,
          sortMode,
          minMatch ?? 0,
        ),
      );
    } finally {
      setLoading(false);
      setBooted(true);
    }
    // workCountry is read server-side; included so changing it re-runs the search.
  }, [
    query,
    workplaces,
    levels,
    selectedKinds,
    relocationOnly,
    remoteEligible,
    postedWithin,
    minSalary,
    langs,
    hideScams,
    sortMode,
    minMatch,
    workCountry,
  ]);

  // --- Per-profile job preferences: each profile remembers its own search filters. ---
  // Apply a saved prefs JSON blob to the filter state (missing keys fall back to defaults),
  // marking skipNextSave so the resulting change is not echoed straight back to disk.
  const applyPrefs = useCallback((json: string) => {
    let p: Partial<JobPrefs> = {};
    try {
      p = json ? (JSON.parse(json) as Partial<JobPrefs>) : {};
    } catch {
      p = {};
    }
    skipNextSave.current = true;
    setWorkplaces(p.workplaces ?? []);
    setLevels(p.levels ?? []);
    setRemoteEligible(p.remoteEligible ?? false);
    setRelocationOnly(p.relocationOnly ?? false);
    setPostedWithin(p.postedWithin ?? null);
    setMinSalary(p.minSalary ?? null);
    setLangs(p.langs ?? []);
    setMinMatch(p.minMatch ?? null);
  }, []);

  // Load the active profile's saved filters once on mount.
  useEffect(() => {
    void (async () => {
      try {
        applyPrefs(await api.getJobPrefs());
      } catch {
        /* no profile yet; leave the defaults */
      }
      prefsReady.current = true;
    })();
  }, [applyPrefs]);

  // Persist filter changes back onto the active profile (debounced). Skips the initial load
  // and any programmatic apply, so loading a profile never echoes a save.
  useEffect(() => {
    if (skipNextSave.current) {
      skipNextSave.current = false;
      return;
    }
    if (!prefsReady.current) return;
    const t = setTimeout(() => {
      const prefs: JobPrefs = {
        workplaces,
        levels,
        remoteEligible,
        relocationOnly,
        postedWithin,
        minSalary,
        langs,
        minMatch,
      };
      void api.setJobPrefs(JSON.stringify(prefs)).catch(() => {});
    }, 400);
    return () => clearTimeout(t);
  }, [workplaces, levels, remoteEligible, relocationOnly, postedWithin, minSalary, langs, minMatch]);

  // When the active profile changes (from the Profile hub), load its filters and re-search:
  // the resume changed too, so match scores differ even if the filters happen to be the same.
  const onProfileChanged = useCallback(async () => {
    try {
      applyPrefs(await api.getJobPrefs());
    } catch {
      /* ignore */
    }
    void runSearch();
  }, [applyPrefs, runSearch]);

  useEffect(() => {
    void refreshSources();
    void refreshAlerts();
    void refreshStats();
    void api
      .workCountry()
      .then(setWorkCountry)
      .catch(() => {});
  }, [refreshSources, refreshAlerts, refreshStats]);

  // Fade out and remove the launch splash once booted; a fallback timer ensures it
  // never sticks if the first load stalls.
  useEffect(() => {
    const hide = () => {
      const el = document.getElementById("jr-splash");
      if (!el) return;
      el.classList.add("jr-hide");
      window.setTimeout(() => el.remove(), 400);
    };
    if (booted) {
      hide();
      return;
    }
    const t = window.setTimeout(hide, 6000);
    return () => window.clearTimeout(t);
  }, [booted]);

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

  useEffect(() => {
    localStorage.setItem("jr.filtersOpen", filtersOpen ? "1" : "0");
  }, [filtersOpen]);

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

  // Only open http(s) links externally. Job/program URLs come from third-party APIs,
  // so reject anything else (file:, javascript:, custom protocol handlers, etc.).
  const onOpen = useCallback((url: string) => {
    try {
      const u = new URL(url);
      if (u.protocol === "http:" || u.protocol === "https:") void openUrl(url);
    } catch {
      /* ignore malformed or unsafe URLs */
    }
  }, []);

  const toggleCapture = useCallback(async () => {
    const next = !capHidden;
    await api.setScreenPrivacy(next);
    setCapHidden(next);
  }, [capHidden]);

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

  // One-click ATS score: score a job's description against the saved resume, shown
  // in a small modal.
  const onScore = useCallback((job: Job) => {
    setScoring({ title: job.title, score: null, loading: true, err: "" });
    void api
      .scoreAts(htmlToText(job.description || ""))
      .then((score) => setScoring({ title: job.title, score, loading: false, err: "" }))
      .catch((e) => setScoring({ title: job.title, score: null, loading: false, err: String(e) }));
  }, []);

  const availableKinds = useMemo(() => Array.from(new Set(sources.map((s) => s.kind))), [sources]);

  const regionNames = useMemo(() => {
    try {
      return new Intl.DisplayNames(["en"], { type: "region" });
    } catch {
      return null;
    }
  }, []);
  const locationName = (workCountry && regionNames?.of(workCountry)) || workCountry || "my location";

  const hasActiveFilters =
    selectedKinds.length > 0 ||
    workplaces.length > 0 ||
    levels.length > 0 ||
    remoteEligible ||
    relocationOnly ||
    postedWithin !== null ||
    minSalary !== null ||
    langs.length > 0 ||
    hideScams ||
    sortMode === "match" ||
    minMatch !== null;

  // Count of active facets, shown as a badge on the collapsed "Filters" pill.
  const activeFilterCount =
    selectedKinds.length +
    workplaces.length +
    levels.length +
    langs.length +
    (remoteEligible ? 1 : 0) +
    (relocationOnly ? 1 : 0) +
    (postedWithin !== null ? 1 : 0) +
    (minSalary !== null ? 1 : 0) +
    (hideScams ? 1 : 0) +
    (sortMode === "match" ? 1 : 0) +
    (minMatch !== null ? 1 : 0);

  const toggle = (setter: Dispatch<SetStateAction<string[]>>, k: string) =>
    setter((prev) => (prev.includes(k) ? prev.filter((x) => x !== k) : [...prev, k]));

  return (
    <div className="app">
      {HAS_TAURI && <ResizeHandles />}
      <header className="topbar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <Radar size={22} className="brand-icon" />
          <span>JobRadar</span>
        </div>
        <nav className="nav">
          <button
            className={"nav-btn" + (view === "search" ? " active" : "")}
            onClick={() => setView("search")}
            title="Search"
          >
            <Search size={15} /> <span className="nav-label">Search</span>
          </button>
          <button
            className={"nav-btn" + (view === "tracker" ? " active" : "")}
            onClick={() => setView("tracker")}
            title="Tracker"
          >
            <ClipboardList size={15} /> <span className="nav-label">Tracker</span>
          </button>
          <button
            className={"nav-btn" + (view === "study" ? " active" : "")}
            onClick={() => setView("study")}
            title="Study"
          >
            <GraduationCap size={15} /> <span className="nav-label">Study</span>
          </button>
          <button
            className={"nav-btn" + (view === "assistant" ? " active" : "")}
            onClick={() => setView("assistant")}
            title="Assistant"
          >
            <Bot size={15} /> <span className="nav-label">Assistant</span>
          </button>
        </nav>
        <div className="topbar-actions">
          <button
            className={"icon-btn" + (capHidden ? " active" : "")}
            title={
              capHidden
                ? "Hidden from screen capture - click to show in shares"
                : "Visible to screen capture - click to hide"
            }
            onClick={toggleCapture}
          >
            {capHidden ? <EyeOff size={18} /> : <Eye size={18} />}
          </button>
          <button className="icon-btn" title="Your profiles" onClick={() => setProfileOpen(true)}>
            <UserRound size={18} />
          </button>
          <button className="icon-btn" title="Getting started" onClick={() => setShowIntro(true)}>
            <HelpCircle size={18} />
          </button>
          <button className="icon-btn" title="Alerts" onClick={() => setPanel("alerts")}>
            <Bell size={18} />
          </button>
          <button className="icon-btn" title="Sources" onClick={() => setPanel("sources")}>
            <Settings size={18} />
          </button>
          <button
            className="btn-sync"
            onClick={onSync}
            disabled={syncing}
            title={syncing ? "Syncing…" : "Sync now"}
          >
            <RefreshCw size={16} className={syncing ? "spin" : ""} />
            <span className="btn-label">{syncing ? "Syncing…" : "Sync now"}</span>
          </button>
          {HAS_TAURI && <WindowControls />}
        </div>
      </header>

      <ProfileHub
        open={profileOpen}
        onClose={() => setProfileOpen(false)}
        onActiveChange={() => void onProfileChanged()}
      />

      {view === "search" ? (
        <>
          <div className="searchbar">
            <div className="search-box">
              <Search size={17} className="search-icon" />
              <input
                autoFocus
                placeholder="Search title, company, location, description…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {query && (
                <button className="search-clear" title="Clear search" onClick={() => setQuery("")}>
                  <X size={15} />
                </button>
              )}
            </div>

            <div className="filter-bar">
              <button
                className={"filter-toggle" + (filtersOpen ? " open" : "")}
                aria-expanded={filtersOpen}
                aria-controls="filter-grid"
                onClick={() => setFiltersOpen((v) => !v)}
              >
                <ChevronDown size={15} className="filter-caret" />
                Filters
                {activeFilterCount > 0 && <span className="filter-count">{activeFilterCount}</span>}
              </button>
            </div>

            {filtersOpen && (
              <div className="filter-grid" id="filter-grid">
                <span className="filter-label">Workplace</span>
                <div className="chip-row">
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

                <span className="filter-label">Level</span>
                <div className="chip-row">
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

                <span className="filter-label">Remote fit</span>
                <div className="chip-row">
                  <button
                    className={"chip" + (remoteEligible ? " active" : "")}
                    title={`Remote roles workable from ${locationName}: hides ones restricted to another region`}
                    onClick={() => setRemoteEligible((v) => !v)}
                  >
                    Workable from <span className="chip-country">{locationName}</span>
                  </button>
                  {editingCountry ? (
                    <input
                      className="country-input"
                      autoFocus
                      maxLength={2}
                      placeholder="MT"
                      defaultValue={workCountry}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                        if (e.key === "Escape") setEditingCountry(false);
                      }}
                      onBlur={async (e) => {
                        const code = e.currentTarget.value.trim();
                        await api.setWorkCountry(code);
                        setWorkCountry(code ? code.toUpperCase() : await api.workCountry());
                        setEditingCountry(false);
                      }}
                    />
                  ) : (
                    <button
                      className="chip-edit"
                      title="Change the country you can work from (2-letter code, blank to auto-detect)"
                      onClick={() => setEditingCountry(true)}
                    >
                      change
                    </button>
                  )}
                  <button
                    className={"chip" + (relocationOnly ? " active" : "")}
                    title="Only jobs whose description mentions relocation help"
                    onClick={() => setRelocationOnly((v) => !v)}
                  >
                    Pays relocation
                  </button>
                  <button
                    className={"chip" + (hideScams ? " active" : "")}
                    title="Hide postings flagged as likely scams (the warning badge still shows on others)"
                    onClick={() => setHideScams((v) => !v)}
                  >
                    Hide likely scams
                  </button>
                </div>

                <span className="filter-label">Posted</span>
                <div className="chip-row">
                  {[
                    { label: "Any time", days: null },
                    { label: "24h", days: 1 },
                    { label: "7 days", days: 7 },
                    { label: "30 days", days: 30 },
                  ].map((o) => (
                    <button
                      key={o.label}
                      className={"chip" + (postedWithin === o.days ? " active" : "")}
                      onClick={() => setPostedWithin(o.days)}
                    >
                      {o.label}
                    </button>
                  ))}
                </div>

                <span className="filter-label">Min salary</span>
                <div className="chip-row">
                  {[
                    { label: "Any", v: null },
                    { label: "50k+", v: 50000 },
                    { label: "80k+", v: 80000 },
                    { label: "100k+", v: 100000 },
                    { label: "150k+", v: 150000 },
                  ].map((o) => (
                    <button
                      key={o.label}
                      className={"chip" + (minSalary === o.v ? " active" : "")}
                      title="Nominal, in the posting's own currency; postings with no listed salary are kept"
                      onClick={() => setMinSalary(o.v)}
                    >
                      {o.label}
                    </button>
                  ))}
                </div>

                <span className="filter-label">Resume match</span>
                <div className="chip-row">
                  <button
                    className={"chip" + (sortMode === "match" ? " active" : "")}
                    title="Sort by fit to your saved resume (set one in the Assistant first)"
                    onClick={() => setSortMode((m) => (m === "match" ? "default" : "match"))}
                  >
                    Best match
                  </button>
                  {[
                    { label: "Any", v: null },
                    { label: "25%+", v: 25 },
                    { label: "50%+", v: 50 },
                    { label: "75%+", v: 75 },
                  ].map((o) => (
                    <button
                      key={o.label}
                      className={"chip" + (minMatch === o.v ? " active" : "")}
                      title="Keep only jobs that cover at least this share of your resume keywords"
                      onClick={() => setMinMatch(o.v)}
                    >
                      {o.label}
                    </button>
                  ))}
                </div>

                <span className="filter-label">Language</span>
                <div className="chip-row">
                  {Object.entries(LANG_LABELS).map(([code, label]) => (
                    <button
                      key={code}
                      className={"chip" + (langs.includes(code) ? " active" : "")}
                      title="Show only these languages; postings whose language can't be detected are kept"
                      onClick={() => toggle(setLangs, code)}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                {availableKinds.length > 0 && (
                  <>
                    <span className="filter-label">Source</span>
                    <div className="chip-row">
                      {availableKinds.map((k) => (
                        <button
                          key={k}
                          className={"chip" + (selectedKinds.includes(k) ? " active" : "")}
                          onClick={() => toggle(setSelectedKinds, k)}
                        >
                          {KIND_LABELS[k] ?? k}
                        </button>
                      ))}
                    </div>
                  </>
                )}
              </div>
            )}

            <div className="searchbar-meta">
              <span className="result-count">
                {loading && jobs.length === 0
                  ? "Searching…"
                  : `${jobs.length} result${jobs.length === 1 ? "" : "s"}`}
              </span>
              {hasActiveFilters && (
                <button
                  className="chip clear"
                  onClick={() => {
                    setSelectedKinds([]);
                    setWorkplaces([]);
                    setLevels([]);
                    setRemoteEligible(false);
                    setRelocationOnly(false);
                    setPostedWithin(null);
                    setMinSalary(null);
                    setLangs([]);
                    setHideScams(false);
                    setSortMode("default");
                    setMinMatch(null);
                  }}
                >
                  <X size={13} /> Clear all
                </button>
              )}
              <span className="corpus-stat">
                {stats.totalJobs.toLocaleString()} jobs · {stats.companies.toLocaleString()} companies in your
                radar
              </span>
            </div>
          </div>

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
                {jobs.map((j) => (
                  <JobCard
                    key={j.id}
                    job={j}
                    kindLabel={KIND_LABELS[j.sourceKind] ?? j.sourceKind}
                    onOpen={onOpen}
                    onTrack={onTrack}
                    onScore={onScore}
                  />
                ))}
              </>
            )}
          </main>
        </>
      ) : (
        <main className="results">
          {/* A distinct key per view forces a fresh boundary on each tab switch, so a
              crash captured in one view never carries over to poison another. */}
          {view === "tracker" ? (
            <ErrorBoundary key="tracker" label="Tracker">
              <TrackerView onOpen={onOpen} />
            </ErrorBoundary>
          ) : view === "study" ? (
            <ErrorBoundary key="study" label="Study view">
              <StudyView />
            </ErrorBoundary>
          ) : (
            <ErrorBoundary key="assistant" label="Assistant">
              <AssistantView />
            </ErrorBoundary>
          )}
        </main>
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

      {showIntro && (
        <GettingStarted
          onClose={dismissIntro}
          onGo={(v) => {
            setView(v);
            dismissIntro();
          }}
        />
      )}

      {scoring && (
        <div className="intro-overlay" onClick={() => setScoring(null)}>
          <div className="score-modal" onClick={(e) => e.stopPropagation()}>
            <button className="intro-close" title="Close" onClick={() => setScoring(null)}>
              <X size={18} />
            </button>
            <h3 className="score-modal-title">{scoring.title}</h3>
            {scoring.loading && <p className="muted sm">Scoring against your saved resume…</p>}
            {scoring.err && <div className="assistant-error">{scoring.err}</div>}
            {scoring.score && <AtsScoreCard score={scoring.score} />}
          </div>
        </div>
      )}
    </div>
  );
}
