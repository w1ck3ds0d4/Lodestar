// A reusable interview story (usually STAR) from the user's real experience.
export interface Story {
  id: number;
  title: string;
  content: string;
  tags: string;
  createdAt: number;
  updatedAt: number;
}

// A saved interview session (transcript + answers) for later review.
export interface Session {
  id: number;
  title: string;
  transcript: string;
  answers: string;
  createdAt: number;
}

// --- Capture mode: state mirrored from the main window to the overlay panels ---
export interface CaptureSegment {
  role: "you" | "them";
  speaker: string;
  text: string;
}
export interface CaptureAnswer {
  q: string;
  a: string;
  pending: boolean;
}
// One activity-log line, so the user can see what each action/shortcut actually did
// (e.g. that "watch" and "read page" are different) and whether it fired.
export interface LogEntry {
  id: number;
  ts: number;
  kind: string;
  msg: string;
}
export interface CaptureState {
  listening: boolean;
  watching: boolean;
  // Whether the panels are hidden from screen capture (false = visible/screenshot-able).
  protect?: boolean;
  segments: CaptureSegment[];
  answers: CaptureAnswer[];
  // Overlay translucency (0.3..1), shared so every opacity control stays in sync.
  opacity?: number;
  // Recent activity log, mirrored to the floating Log panel.
  log?: LogEntry[];
  // Active answer-style id (e.g. "auto", "deep"), so the Controls panel can show/change it.
  answerStyle?: string;
}
// A command sent from an overlay window (panel or control bar) to the main window.
export interface CaptureCommand {
  cmd:
    | "listen"
    | "watch"
    | "snip"
    | "ask"
    | "stop"
    | "sync"
    | "answer_screen"
    | "protect"
    | "read_page"
    | "read_full_page"
    | "opacity"
    | "style";
  text?: string;
  value?: number; // for "opacity" (0..1)
  style?: string; // for "style" (an ANSWER_STYLES id)
}

export interface AssistantSettings {
  hasKey: boolean;
  hasSttKey: boolean;
  sttEngine: string;
  sttLanguage: string;
  sttDiarize: boolean;
  captureSource: string;
  model: string;
  inputDevice: string;
  outputDevice: string;
  answerEngine: string;
  answerStyle: string;
  ollamaUrl: string;
  ollamaModel: string;
  contextRole: string;
  contextCompany: string;
  contextResume: string;
  contextNotes: string;
  captureProtect: boolean; // capture panels hidden from screen capture (default true)
}

export interface AudioDevices {
  inputs: string[];
  outputs: string[];
}

export interface MonitorInfo {
  id: number;
  name: string;
}

export interface ScreenAnswer {
  changed: boolean;
  answer: string;
}

// Structured ATS-style resume/JD match score. `overall` is a weighted blend of the
// sub-scores (each 0-100), computed on the backend.
export interface AtsScore {
  overall: number;
  keywords: number;
  responsibilities: number;
  title: number;
  impact: number;
  format: number;
  matched: string[];
  missing: string[];
  fixes: string[];
}

// Deepgram transcription languages. "multi" is the multilingual / code-switching
// model; the rest pin a single language (streaming has no auto-detect, unlike batch).
export const STT_LANGUAGES: { code: string; label: string }[] = [
  { code: "multi", label: "Multilingual (auto)" },
  { code: "en", label: "English" },
  { code: "pt", label: "Portuguese" },
  { code: "es", label: "Spanish" },
  { code: "fr", label: "French" },
  { code: "de", label: "German" },
  { code: "it", label: "Italian" },
  { code: "nl", label: "Dutch" },
  { code: "ru", label: "Russian" },
  { code: "hi", label: "Hindi" },
  { code: "ja", label: "Japanese" },
  { code: "ko", label: "Korean" },
  { code: "zh", label: "Chinese" },
];

// How answers are formatted. "auto" lets the model choose per question.
export const ANSWER_STYLES: { id: string; label: string; note: string }[] = [
  { id: "auto", label: "Auto", note: "best format per question" },
  { id: "concise", label: "Concise", note: "glanceable bullets" },
  { id: "deep", label: "Deep dive", note: "walk through the reasoning, for technical panels" },
  { id: "star", label: "STAR", note: "behavioral structure" },
  { id: "code", label: "Code", note: "solution + complexity" },
  { id: "code-steps", label: "Code steps", note: "build it as you type, full code at the end" },
  { id: "narrate", label: "Narrate", note: "plan + spoken 'what I'm doing' for live coding" },
];

// `inPrice`/`outPrice` are USD per million tokens, for the in-app cost estimate.
// Approximate published Claude pricing; the real bill lives in the Anthropic console.
export const ASSISTANT_MODELS: {
  id: string;
  label: string;
  note: string;
  inPrice: number;
  outPrice: number;
}[] = [
  // Current generation. Prices are Anthropic list rates, USD per 1M tokens.
  { id: "claude-opus-5", label: "Claude Opus 5", note: "most capable", inPrice: 5, outPrice: 25 },
  { id: "claude-sonnet-5", label: "Claude Sonnet 5", note: "balanced, fast", inPrice: 2, outPrice: 10 },
  { id: "claude-haiku-4-5", label: "Claude Haiku 4.5", note: "fastest, cheapest", inPrice: 1, outPrice: 5 },
  // Deepest reasoning, but turns can run minutes - poor fit for answering live.
  {
    id: "claude-fable-5-1",
    label: "Claude Fable 5.1",
    note: "deepest, slowest",
    inPrice: 10,
    outPrice: 50,
  },
  // Previous generation, kept so saved settings keep working.
  { id: "claude-opus-4-8", label: "Claude Opus 4.8", note: "previous flagship", inPrice: 5, outPrice: 25 },
  { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6", note: "balanced (older)", inPrice: 3, outPrice: 15 },
];

export interface ClaudeUsageItem {
  model: string;
  input: number;
  output: number;
}

export interface UsageReport {
  since: number | null; // unix seconds the tally started
  claudeItems: ClaudeUsageItem[];
  deepgramSeconds: number;
  claudeCredit: number; // user-entered top-up, USD (0 if unset)
  deepgramCredit: number;
  // Manual "actual spent" corrections, USD. When set, the UI shows these instead of the
  // estimate so the figures can be synced to the provider console. null = use estimate.
  claudeSpentOverride: number | null;
  deepgramSpentOverride: number | null;
}

export interface DeepgramBalance {
  amount: number;
  units: string;
  project: string;
}

// Connection details for the phone companion server.
export interface PhoneInfo {
  url: string;
  pin: string;
  qr: string; // SVG data: URL
}

// A switchable interview persona: the context the assistant uses to tailor live answers.
export interface Profile {
  id: number;
  name: string;
  role: string;
  company: string;
  resume: string;
  notes: string;
  createdAt: number;
  updatedAt: number;
}

// Result of the networking/firewall pre-flight for the phone server (Windows).
export interface PhoneNetStatus {
  ok: boolean; // nothing obvious should block the phone
  publicNetwork: boolean; // network is categorized Public (blocks inbound LAN)
  ruleMissing: boolean; // no inbound firewall allow-rule for the port
  canFix: boolean; // an automatic, one-click fix is available on this platform
  message: string; // user-facing explanation
}

// Deepgram Nova-2 streaming, approximate USD per minute (pay-as-you-go). Estimate only.
export const DEEPGRAM_PER_MIN = 0.0059;

// Estimated USD cost of one Claude usage row. Unknown models fall back to Sonnet pricing.
export function estimateCost(item: ClaudeUsageItem): number {
  const m = ASSISTANT_MODELS.find((x) => x.id === item.model);
  const inPrice = m?.inPrice ?? 3;
  const outPrice = m?.outPrice ?? 15;
  return (item.input / 1_000_000) * inPrice + (item.output / 1_000_000) * outPrice;
}
