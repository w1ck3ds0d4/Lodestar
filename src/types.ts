export interface Job {
  id: number;
  sourceKind: string;
  sourceLabel: string;
  externalId: string;
  title: string;
  company: string;
  location: string;
  remote: boolean;
  workplace: string;
  experienceLevel: string;
  employmentType: string | null;
  description: string;
  url: string;
  salary: string | null;
  postedAt: string | null;
  relocation: boolean;
  remoteRegion: string;
  firstSeenAt: number;
  lastSeenAt: number;
}

export interface Source {
  id: number;
  kind: string;
  label: string;
  config: Record<string, string>;
  enabled: boolean;
  minIntervalSecs: number;
  lastRunAt: number | null;
  lastStatus: string | null;
  lastCount: number | null;
}

export interface Alert {
  id: number;
  query: string;
  remoteOnly: boolean;
  lastNotifiedAt: number | null;
}

export interface Stats {
  totalJobs: number;
  companies: number;
}

export interface Milestone {
  id: number;
  trackedId: number;
  label: string;
  done: boolean;
  position: number;
}

export interface Tracked {
  id: number;
  kind: string; // "application" | "program"
  title: string;
  org: string;
  url: string;
  status: string;
  notes: string;
  appliedAt: string | null;
  deadline: string | null;
  progress: number;
  sourceJobId: number | null;
  nextAction: string | null;
  prep: string | null;
  createdAt: number;
  updatedAt: number;
  milestones: Milestone[];
}

export interface Skill {
  id: number;
  name: string;
  goal: string;
  level: number;
  hours: number;
  notes: string;
  createdAt: number;
  updatedAt: number;
}

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

// A spaced-repetition flashcard (Leitner). dueAt is Unix ms.
export interface Card {
  id: number;
  front: string;
  back: string;
  box: number;
  dueAt: number;
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
export interface CaptureState {
  listening: boolean;
  watching: boolean;
  segments: CaptureSegment[];
  answers: CaptureAnswer[];
}
// A command sent from an overlay window (panel or control bar) to the main window.
export interface CaptureCommand {
  cmd: "listen" | "watch" | "snip" | "ask" | "stop" | "sync" | "answer_screen";
  text?: string;
}

export const APP_STATUSES = ["saved", "applied", "screen", "interview", "offer", "accepted", "rejected"] as const;
export const APP_STATUS_LABELS: Record<string, string> = {
  saved: "Saved",
  applied: "Applied",
  screen: "Phone screen",
  interview: "Interview",
  offer: "Offer",
  accepted: "Accepted",
  rejected: "Rejected",
};

export const PROGRAM_STATUSES = ["interested", "enrolled", "in_progress", "completed", "dropped"] as const;
export const PROGRAM_STATUS_LABELS: Record<string, string> = {
  interested: "Interested",
  enrolled: "Enrolled",
  in_progress: "In progress",
  completed: "Completed",
  dropped: "Dropped",
};

export interface CatalogProgram {
  name: string;
  org: string;
  url: string;
  tag: string;
}

// Pre-seeded programs/platforms the user can add to their tracker with one click.
export const PROGRAM_CATALOG: CatalogProgram[] = [
  { name: "Crossover", org: "Crossover", url: "https://www.crossover.com/", tag: "Remote marketplace" },
  { name: "Turing", org: "Turing", url: "https://www.turing.com/", tag: "Remote marketplace" },
  { name: "Andela", org: "Andela", url: "https://www.andela.com/", tag: "Remote marketplace" },
  { name: "Toptal", org: "Toptal", url: "https://www.toptal.com/", tag: "Remote marketplace" },
  { name: "Arc.dev", org: "Arc", url: "https://arc.dev/", tag: "Remote marketplace" },
  { name: "LinkedIn REACH", org: "LinkedIn", url: "https://careers.linkedin.com/reach", tag: "Paid apprenticeship" },
  {
    name: "Amazon Technical Academy",
    org: "Amazon",
    url: "https://www.amazon.jobs/content/en/teams/amazon-web-services/technical-apprenticeships",
    tag: "Paid apprenticeship",
  },
  { name: "Microsoft LEAP", org: "Microsoft", url: "https://leap.microsoft.com/", tag: "Paid apprenticeship" },
  { name: "Multiverse", org: "Multiverse", url: "https://www.multiverse.io/", tag: "Apprenticeships" },
  { name: "Year Up", org: "Year Up United", url: "https://www.yearup.org/", tag: "Free + stipend" },
  { name: "Per Scholas", org: "Per Scholas", url: "https://perscholas.org/", tag: "Free + stipend" },
  { name: "apprenticeship.gov", org: "US DOL", url: "https://www.apprenticeship.gov/", tag: "Registered apprenticeships" },
];

export const SOURCE_KINDS = [
  "greenhouse",
  "lever",
  "ashby",
  "arbeitnow",
  "remotive",
  "jobicy",
  "jsonld",
  "adzuna",
  "usajobs",
] as const;

export const WORKPLACES = [
  { key: "remote", label: "Remote" },
  { key: "hybrid", label: "Hybrid" },
  { key: "onsite", label: "On-site" },
] as const;

export const LEVELS = [
  { key: "entry", label: "Entry / Junior" },
  { key: "mid", label: "Mid" },
  { key: "senior", label: "Senior" },
] as const;

export const WORKPLACE_LABELS: Record<string, string> = {
  remote: "Remote",
  hybrid: "Hybrid",
  onsite: "On-site",
};

// Best-effort remote-eligibility region shown on the job card.
export const REMOTE_REGION_LABELS: Record<string, string> = {
  worldwide: "Remote · Worldwide",
  eu: "Remote · EU",
  us: "Remote · US only",
  uk: "Remote · UK only",
  other: "Remote · region-locked",
};

export const LEVEL_LABELS: Record<string, string> = {
  entry: "Entry level",
  mid: "Mid level",
  senior: "Senior",
};

export const KIND_LABELS: Record<string, string> = {
  greenhouse: "Greenhouse",
  lever: "Lever",
  ashby: "Ashby",
  arbeitnow: "Arbeitnow",
  remotive: "Remotive",
  jobicy: "Jobicy",
  jsonld: "Career page",
  adzuna: "Adzuna",
  usajobs: "USAJOBS",
};

// Per-kind config fields shown in the Sources panel.
export const CONFIG_FIELDS: Record<string, { key: string; label: string; placeholder?: string }[]> = {
  greenhouse: [{ key: "company", label: "Board token", placeholder: "stripe" }],
  lever: [{ key: "company", label: "Company", placeholder: "company-slug" }],
  ashby: [
    { key: "name", label: "Job board name", placeholder: "company" },
    { key: "company", label: "Company label (optional)" },
  ],
  jsonld: [{ key: "url", label: "Careers page URL", placeholder: "https://example.com/careers" }],
  adzuna: [
    { key: "app_id", label: "App ID" },
    { key: "app_key", label: "App key" },
    { key: "country", label: "Country", placeholder: "gb" },
    { key: "what", label: "Keywords (optional)" },
    { key: "where", label: "Location (optional)" },
  ],
  usajobs: [
    { key: "email", label: "Email (User-Agent)" },
    { key: "api_key", label: "API key" },
    { key: "keyword", label: "Keyword (optional)" },
  ],
  arbeitnow: [],
  remotive: [],
  jobicy: [],
};

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

// `inPrice`/`outPrice` are USD per million tokens, for the in-app cost estimate.
// Approximate published Claude pricing; the real bill lives in the Anthropic console.
// How answers are formatted. "auto" lets the model choose per question.
export const ANSWER_STYLES: { id: string; label: string; note: string }[] = [
  { id: "auto", label: "Auto", note: "best format per question" },
  { id: "concise", label: "Concise", note: "glanceable bullets" },
  { id: "star", label: "STAR", note: "behavioral structure" },
  { id: "code", label: "Code", note: "solution + complexity" },
];

export const ASSISTANT_MODELS: { id: string; label: string; note: string; inPrice: number; outPrice: number }[] = [
  { id: "claude-opus-4-8", label: "Claude Opus 4.8", note: "most capable, slower", inPrice: 15, outPrice: 75 },
  { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6", note: "balanced", inPrice: 3, outPrice: 15 },
  { id: "claude-haiku-4-5", label: "Claude Haiku 4.5", note: "fastest", inPrice: 1, outPrice: 5 },
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

// Deepgram Nova-2 streaming, approximate USD per minute (pay-as-you-go). Estimate only.
export const DEEPGRAM_PER_MIN = 0.0059;

// Estimated USD cost of one Claude usage row. Unknown models fall back to Sonnet pricing.
export function estimateCost(item: ClaudeUsageItem): number {
  const m = ASSISTANT_MODELS.find((x) => x.id === item.model);
  const inPrice = m?.inPrice ?? 3;
  const outPrice = m?.outPrice ?? 15;
  return (item.input / 1_000_000) * inPrice + (item.output / 1_000_000) * outPrice;
}
