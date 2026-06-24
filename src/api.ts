import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  Alert,
  AssistantSettings,
  AtsScore,
  AudioDevices,
  Card,
  DeepgramBalance,
  UsageReport,
  Job,
  MonitorInfo,
  PhoneInfo,
  ScreenAnswer,
  Session,
  Skill,
  Source,
  Stats,
  Story,
  Tracked,
} from "./types";

// --- Browser-preview fallback -------------------------------------------------
// Outside Tauri (the Vite dev server in a plain browser, e.g. the Claude Code
// preview) there is no Rust backend, so a real `invoke` would hang and the app
// would stick on the launch splash. This shim returns type-shaped empty defaults
// so the UI boots and renders its empty states for styling work. It is inert in
// the desktop app, where `__TAURI_INTERNALS__` is present.
const HAS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const PREVIEW_STUBS: Record<string, unknown> = {
  search_jobs: [],
  work_country: "",
  list_sources: [],
  list_alerts: [],
  get_stats: { totalJobs: 0, companies: 0 },
  list_tracked: [],
  list_skills: [],
  list_stories: [],
  list_cards: [],
  list_sessions: [],
  list_ollama_models: [],
  list_audio_devices: { inputs: [], outputs: [] },
  list_monitors: [],
  get_assistant_settings: {
    // Preview shows the full Assistant UI (the real app gates this on a saved key).
    hasKey: true,
    hasSttKey: false,
    sttEngine: "deepgram",
    sttLanguage: "multi",
    sttDiarize: false,
    captureSource: "system",
    model: "claude-sonnet-4-6",
    inputDevice: "",
    outputDevice: "",
    answerEngine: "claude",
    answerStyle: "auto",
    ollamaUrl: "http://127.0.0.1:11434",
    ollamaModel: "",
    contextRole: "",
    contextCompany: "",
    contextResume: "",
    contextNotes: "",
    captureProtect: true,
  },
  get_usage: {
    since: null,
    claudeItems: [],
    deepgramSeconds: 0,
    claudeCredit: 0,
    deepgramCredit: 0,
    claudeSpentOverride: null,
    deepgramSpentOverride: null,
  },
  deepgram_balance: { amount: 0, units: "USD", project: "" },
};

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (HAS_TAURI) return tauriInvoke<T>(cmd, args);
  return Promise.resolve((cmd in PREVIEW_STUBS ? PREVIEW_STUBS[cmd] : undefined) as T);
}

export const searchJobs = (
  query: string,
  workplaces: string[],
  levels: string[],
  kinds: string[],
  relocationOnly = false,
  remoteEligible = false,
  postedWithinDays: number | null = null,
  minSalary: number | null = null,
  langs: string[] = [],
  hideScams = false,
  sort: "default" | "match" = "default",
  minMatch = 0,
  limit = 200,
) =>
  invoke<Job[]>("search_jobs", {
    query,
    workplaces,
    levels,
    kinds,
    relocationOnly,
    remoteEligible,
    postedWithinDays,
    minSalary,
    langs,
    hideScams,
    sort,
    minMatch,
    limit,
  });

// Detected (or stored) ISO country code the user can work from; "" if unknown.
export const workCountry = () => invoke<string>("work_country");
export const setWorkCountry = (code: string) => invoke<void>("set_work_country", { code });

export const listSources = () => invoke<Source[]>("list_sources");

export const addSource = (kind: string, label: string, config: Record<string, string>, minIntervalSecs: number) =>
  invoke<number>("add_source", { kind, label, config, minIntervalSecs });

export const updateSource = (
  id: number,
  label: string,
  enabled: boolean,
  config: Record<string, string>,
  minIntervalSecs: number,
) => invoke<void>("update_source", { id, label, enabled, config, minIntervalSecs });

export const removeSource = (id: number) => invoke<void>("remove_source", { id });

export const getStats = () => invoke<Stats>("get_stats");

export const listAlerts = () => invoke<Alert[]>("list_alerts");

export const addAlert = (query: string, remoteOnly: boolean) =>
  invoke<number>("add_alert", { query, remoteOnly });

export const removeAlert = (id: number) => invoke<void>("remove_alert", { id });

export const syncNow = () => invoke<void>("sync_now");

// Fetch a single source right now; resolves to the job count, rejects with a
// (redacted) error string. Used by the Sources panel "Test" button.
export const runSource = (id: number) => invoke<number>("run_source", { id });

// --- Tracker ---
export const listTracked = (kind?: string) => invoke<Tracked[]>("list_tracked", { kind: kind ?? null });

export const addTracked = (t: {
  kind: string;
  title: string;
  org: string;
  url: string;
  status: string;
  notes: string;
  deadline: string | null;
  sourceJobId: number | null;
}) => invoke<number>("add_tracked", t);

export const updateTracked = (id: number, status: string, notes: string, progress: number, deadline: string | null) =>
  invoke<void>("update_tracked", { id, status, notes, progress, deadline });

export const removeTracked = (id: number) => invoke<void>("remove_tracked", { id });
export const setTrackedNextAction = (id: number, text: string) =>
  invoke<void>("set_tracked_next_action", { id, text });
export const prepApplication = (id: number, title: string, org: string) =>
  invoke<string>("prep_application", { id, title, org });

export const addMilestone = (trackedId: number, label: string) => invoke<number>("add_milestone", { trackedId, label });
export const setMilestone = (id: number, done: boolean) => invoke<void>("set_milestone", { id, done });
export const removeMilestone = (id: number) => invoke<void>("remove_milestone", { id });

// --- Study / skills ---
export const listSkills = () => invoke<Skill[]>("list_skills");
export const addSkill = (name: string, goal: string) => invoke<number>("add_skill", { name, goal });
export const updateSkill = (id: number, name: string, goal: string, level: number, notes: string) =>
  invoke<void>("update_skill", { id, name, goal, level, notes });
export const logStudy = (id: number, hours: number) => invoke<void>("log_study", { id, hours });
export const removeSkill = (id: number) => invoke<void>("remove_skill", { id });
export const studyPlan = (skill: string, goal: string) => invoke<string>("study_plan", { skill, goal });

// --- Interview story bank ---
export const listStories = () => invoke<Story[]>("list_stories");
export const addStory = (title: string, content: string, tags: string) =>
  invoke<number>("add_story", { title, content, tags });
export const updateStory = (id: number, title: string, content: string, tags: string) =>
  invoke<void>("update_story", { id, title, content, tags });
export const removeStory = (id: number) => invoke<void>("remove_story", { id });
export const draftStory = (notes: string) => invoke<string>("draft_story", { notes });

// --- Application kit: tailor resume/cover/ATS/thank-you to a job ---
// kind: "bullets" | "cover" | "ats" | "thanks"; input is the JD (or transcript for "thanks").
export const applicationDoc = (kind: string, input: string) =>
  invoke<string>("application_doc", { kind, input });
// Structured ATS score of the stored resume/profile against a job description.
export const scoreAts = (jd: string) => invoke<AtsScore>("score_ats", { jd });

// --- Mock interview practice ---
export const practiceQuestion = (asked: string[]) => invoke<string>("practice_question", { asked });
export const practiceFeedback = (question: string, answer: string) =>
  invoke<string>("practice_feedback", { question, answer });

// --- Flashcards (spaced repetition) ---
export const listCards = () => invoke<Card[]>("list_cards");
export const generateCards = (source: string) => invoke<number>("generate_cards", { source });
export const reviewCard = (id: number, good: boolean) => invoke<void>("review_card", { id, good });
export const removeCard = (id: number) => invoke<void>("remove_card", { id });

// --- Interview session history ---
export const listSessions = () => invoke<Session[]>("list_sessions");
export const saveSession = (title: string, transcript: string, answers: string) =>
  invoke<number>("save_session", { title, transcript, answers });
export const removeSession = (id: number) => invoke<void>("remove_session", { id });
export const sessionReview = (transcript: string, answers: string) =>
  invoke<string>("session_review", { transcript, answers });
// Export a session's transcript + answers to a Markdown file (Downloads/JobRadar);
// resolves to the file path written.
export const exportSession = (id: number) => invoke<string>("export_session", { id });
// Export every saved session; resolves to { dir, count }.
export const exportAllSessions = () =>
  invoke<{ dir: string; count: number }>("export_all_sessions");

// --- Screen privacy (exclude window from screen capture / sharing) ---
export const setScreenPrivacy = (enabled: boolean) => invoke<void>("set_screen_privacy", { enabled });

// --- Assistant (Claude API) ---
export const getAssistantSettings = () => invoke<AssistantSettings>("get_assistant_settings");
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });
export const setAssistantModel = (model: string) => invoke<void>("set_assistant_model", { model });
// Answer engine: Claude (cloud, default) vs local Ollama.
export const setAnswerEngine = (engine: string) => invoke<void>("set_answer_engine", { engine });
export const setAnswerStyle = (style: string) => invoke<void>("set_answer_style", { style });
export const setOllamaModel = (model: string) => invoke<void>("set_ollama_model", { model });
export const setOllamaUrl = (url: string) => invoke<void>("set_ollama_url", { url });
export const listOllamaModels = () => invoke<string[]>("list_ollama_models");
// Interview/study context (role, resume, notes) used to tailor answers.
export const setInterviewContext = (role: string, company: string, resume: string, notes: string) =>
  invoke<void>("set_interview_context", { role, company, resume, notes });
// Streamed answer chunks from `ask_assistant` (over a Tauri channel).
export type AnswerChunk =
  | { kind: "delta"; text: string }
  | { kind: "done"; text: string }
  | { kind: "error"; message: string };
export const askAssistant = (question: string, context: string | null, onEvent: Channel<AnswerChunk>) =>
  invoke<void>("ask_assistant", { question, context, onEvent });
// Streams the auto-answer over a channel (delta/done/error), same shape as askAssistant.
// The full text is "NONE" when nothing is being asked - the caller suppresses it.
export const autoAnswer = (transcript: string, latest: string, onEvent: Channel<AnswerChunk>) =>
  invoke<void>("auto_answer", { transcript, latest, onEvent });

// Read plain text from the system clipboard (the "Read page" context grab).
export const readClipboard = () => invoke<string>("read_clipboard");

// --- Usage / cost ---
export const getUsage = () => invoke<UsageReport>("get_usage");
export const resetUsage = () => invoke<void>("reset_usage");
export const setUsageCredit = (provider: "claude" | "deepgram", amount: number) =>
  invoke<void>("set_usage_credit", { provider, amount });
// Override the "actual spent" figure to match the provider console; a negative amount
// clears the override (back to the local estimate).
export const setUsageSpent = (provider: "claude" | "deepgram", amount: number) =>
  invoke<void>("set_usage_spent", { provider, amount });
export const deepgramBalance = () => invoke<DeepgramBalance>("deepgram_balance");

// --- Live listening (system-audio capture -> Deepgram) ---
export const setDeepgramKey = (key: string) => invoke<void>("set_deepgram_key", { key });
export const setSttEngine = (engine: string) => invoke<void>("set_stt_engine", { engine });
export const setSttLanguage = (language: string) => invoke<void>("set_stt_language", { language });
export const setSttDiarize = (diarize: boolean) => invoke<void>("set_stt_diarize", { diarize });
export const setCaptureSource = (source: string) => invoke<void>("set_capture_source", { source });
export const listAudioDevices = () => invoke<AudioDevices>("list_audio_devices");
export const setInputDevice = (name: string) => invoke<void>("set_input_device", { name });
export const setOutputDevice = (name: string) => invoke<void>("set_output_device", { name });
export const startListening = () => invoke<void>("start_listening");
export const stopListening = () => invoke<void>("stop_listening");

// --- Phone companion (LAN server: view + control from a phone) ---
export const startPhoneServer = () => invoke<PhoneInfo>("start_phone_server");
export const stopPhoneServer = () => invoke<void>("stop_phone_server");
export const phoneBroadcast = (payload: string) => invoke<void>("phone_broadcast", { payload });

// --- Capture mode (floating overlay panels) ---
export const openCapturePanels = () => invoke<void>("open_capture_panels");
export const closeCapturePanels = () => invoke<void>("close_capture_panels");
// Whether the capture panels are hidden from screen capture/sharing. Pass false to make
// them appear in screenshots/shares (so you can capture the panels); true re-hides them.
export const setCaptureProtection = (protected_: boolean) =>
  invoke<void>("set_capture_protection", { protected: protected_ });
// Show/hide the panels (control bar stays); resolves to the new visibility.
export const toggleCapturePanels = () => invoke<boolean>("toggle_capture_panels");

// --- Screen watcher (capture a monitor -> Claude vision) ---
export const listMonitors = () => invoke<MonitorInfo[]>("list_monitors");
export const answerScreen = (monitor: number, force: boolean, context: string | null = null) =>
  invoke<ScreenAnswer>("answer_screen", { monitor, force, context });
// Snip-to-explain: grab the monitor as a base64 PNG, crop client-side, explain the crop.
export const captureScreenPng = (monitor: number) => invoke<string>("capture_screen_png", { monitor });
export const explainRegion = (pngBase64: string) => invoke<string>("explain_region", { pngBase64 });

// --- Overlay window controls (translucency + always-on-top; Windows-backed) ---
export const setWindowOpacity = (opacity: number) => invoke<void>("set_window_opacity", { opacity });
export const setWindowAlwaysOnTop = (onTop: boolean) =>
  invoke<void>("set_window_always_on_top", { onTop });
