import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  AssistantSettings,
  AtsScore,
  AudioDevices,
  Debrief,
  DeepgramBalance,
  MonitorInfo,
  PhoneInfo,
  PhoneNetStatus,
  Profile,
  ScreenAnswer,
  Session,
  Story,
  UsageReport,
} from "./types";

// --- Browser-preview fallback -------------------------------------------------
// Outside Tauri (the Vite dev server in a plain browser, e.g. the Claude Code
// preview) there is no Rust backend, so a real `invoke` would hang and the app
// would stick on the launch splash. This shim returns type-shaped empty defaults
// so the UI boots and renders its empty states for styling work. It is inert in
// the desktop app, where `__TAURI_INTERNALS__` is present.
const HAS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const PREVIEW_STUBS: Record<string, unknown> = {
  check_claims: "NONE",
  debrief: { topics: [], sessionsTotal: 0, sessionsAnalyzed: 0 },
  list_stories: [],
  list_profiles: [],
  active_profile: 0,
  read_clipboard: "",
  read_active_page: "",
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
    fastMode: false,
    overclaimGuard: false,
    predictFollowups: true,
    captureSource: "system",
    model: "claude-opus-5",
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


// Detected (or stored) ISO country code the user can work from; "" if unknown.










// Fetch a single source right now; resolves to the job count, rejects with a
// (redacted) error string. Used by the Sources panel "Test" button.

// --- Tracker ---





// --- Study / skills ---

// --- Interview story bank ---
export const listStories = () => invoke<Story[]>("list_stories");
export const addStory = (title: string, content: string, tags: string) =>
  invoke<number>("add_story", { title, content, tags });
export const updateStory = (id: number, title: string, content: string, tags: string) =>
  invoke<void>("update_story", { id, title, content, tags });
export const removeStory = (id: number) => invoke<void>("remove_story", { id });
export const draftStory = (notes: string) => invoke<string>("draft_story", { notes });

// --- Profiles (switchable interview personas; the active one drives the assistant) ---
export const listProfiles = () => invoke<Profile[]>("list_profiles");
export const activeProfile = () => invoke<number>("active_profile");
export const setActiveProfile = (id: number) => invoke<void>("set_active_profile", { id });
export const createProfile = (name: string) => invoke<Profile>("create_profile", { name });
export const updateProfile = (
  id: number,
  name: string,
  role: string,
  company: string,
  resume: string,
  notes: string,
) => invoke<void>("update_profile", { id, name, role, company, resume, notes });
export const deleteProfile = (id: number) => invoke<void>("delete_profile", { id });
// Per-profile job-search preferences, stored as an opaque JSON string the frontend owns.

// --- Application kit: tailor resume/cover/ATS/thank-you to a job ---
// kind: "bullets" | "cover" | "ats" | "thanks"; input is the JD (or transcript for "thanks").
export const applicationDoc = (kind: string, input: string) =>
  invoke<string>("application_doc", { kind, input });
// Structured ATS score of the stored resume/profile against a job description.
export const scoreAts = (jd: string) => invoke<AtsScore>("score_ats", { jd });

// --- Mock interview practice ---

// --- Flashcards (spaced repetition) ---

// --- Interview session history ---
export const listSessions = () => invoke<Session[]>("list_sessions");
export const saveSession = (title: string, transcript: string, answers: string) =>
  invoke<number>("save_session", { title, transcript, answers });
export const removeSession = (id: number) => invoke<void>("remove_session", { id });
export const debrief = () => invoke<Debrief>("debrief");
export const sessionReview = (transcript: string, answers: string) =>
  invoke<string>("session_review", { transcript, answers });
// Export a session's transcript + answers to a Markdown file (Downloads/Lodestar);
// resolves to the file path written.
export const exportSession = (id: number) => invoke<string>("export_session", { id });
// Export every saved session; resolves to { dir, count }.
export const exportAllSessions = () => invoke<{ dir: string; count: number }>("export_all_sessions");

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
  { kind: "delta"; text: string } | { kind: "done"; text: string } | { kind: "error"; message: string };
export const askAssistant = (question: string, context: string | null, onEvent: Channel<AnswerChunk>) =>
  invoke<void>("ask_assistant", { question, context, onEvent });
// Ask about an attached image (base64 PNG, no data: prefix) with an optional text question.
export const askImage = (
  question: string,
  image: string,
  context: string | null,
  onEvent: Channel<AnswerChunk>,
) => invoke<void>("ask_image", { question, image, context, onEvent });
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
export const checkClaims = (answer: string) => invoke<string>("check_claims", { answer });
export const setPredictFollowups = (on: boolean) => invoke<void>("set_predict_followups", { on });
export const setOverclaimGuard = (on: boolean) => invoke<void>("set_overclaim_guard", { on });
export const setFastMode = (fast: boolean) => invoke<void>("set_fast_mode", { fast });
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
// Pre-flight: is Windows networking/firewall likely to block the phone, and can we fix it?
export const phoneNetworkStatus = () => invoke<PhoneNetStatus>("phone_network_status");
export const phoneAllowFirewall = () => invoke<PhoneNetStatus>("phone_allow_firewall");

// --- Capture mode (floating overlay panels) ---
export const openCapturePanels = (monitor: number) => invoke<void>("open_capture_panels", { monitor });
export const closeCapturePanels = () => invoke<void>("close_capture_panels");
// Translucency (0..1) of the whole capture overlay (bar + panels) together.
export const setCaptureOpacity = (opacity: number) => invoke<void>("set_capture_opacity", { opacity });
// Whether the capture panels are hidden from screen capture/sharing. Pass false to make
// them appear in screenshots/shares (so you can capture the panels); true re-hides them.
export const setCaptureProtection = (protected_: boolean) =>
  invoke<void>("set_capture_protection", { protected: protected_ });
// Show/hide the panels (control bar stays); resolves to the new visibility.
export const toggleCapturePanels = () => invoke<boolean>("toggle_capture_panels");
// Move or resize a panel ("all" or a cap-* label) from the phone: snap, nudge, or grow/shrink.
export const moveCapturePanel = (panel: string, snap?: string, nudge?: string, resize?: string) =>
  invoke<void>("move_capture_panel", {
    panel,
    snap: snap ?? null,
    nudge: nudge ?? null,
    resize: resize ?? null,
  });
// Read the full text of the page the user is on (incl. scrolled-off content) for context.
export const readActivePage = () => invoke<string>("read_active_page");

// --- Screen watcher (capture a monitor -> Claude vision) ---
export const listMonitors = () => invoke<MonitorInfo[]>("list_monitors");
export const answerScreen = (monitor: number, force: boolean, context: string | null = null) =>
  invoke<ScreenAnswer>("answer_screen", { monitor, force, context });
// Hybrid: full focused-page text (scroll-independent) plus the screenshot, for the phone remote.
export const answerScreenHybrid = (monitor: number, force: boolean, context: string | null = null) =>
  invoke<ScreenAnswer>("answer_screen_hybrid", { monitor, force, context });
// Snip-to-explain: grab the monitor as a base64 PNG, crop client-side, explain the crop.
export const captureScreenPng = (monitor: number) => invoke<string>("capture_screen_png", { monitor });
export const explainRegion = (pngBase64: string) => invoke<string>("explain_region", { pngBase64 });

// --- Overlay window controls (translucency + always-on-top; Windows-backed) ---
export const setWindowOpacity = (opacity: number) => invoke<void>("set_window_opacity", { opacity });
export const setWindowAlwaysOnTop = (onTop: boolean) => invoke<void>("set_window_always_on_top", { onTop });
export const setWindowSkipTaskbar = (skip: boolean) => invoke<void>("set_window_skip_taskbar", { skip });
// (Re)apply the full taskbar / Alt+Tab / background-process stealth. Called once on app mount so it
// holds on every view, not just the Assistant tab where the on-top / skip-taskbar toggles live.
export const applyWindowStealth = () => invoke<void>("apply_window_stealth");
