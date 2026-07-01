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
  alsoOn: string[];
  lang: string | null;
  // Advisory scam score (0-100): >=40 "review", >=75 "likely scam". Never auto-hides.
  scamScore: number;
  // The scam check ids that fired, for the "why flagged" tooltip.
  scamFlags: string[];
  // Fit to the saved resume (0-100), computed at search time; 0 when no resume is set.
  matchScore: number;
}

// Scam-score badge tiers (mirror the Rust thresholds in scam.rs).
export const SCAM_REVIEW = 40;
export const SCAM_LIKELY = 75;

// Human labels for the scam check ids stored in Job.scamFlags (for the tooltip).
export const SCAM_FLAG_LABELS: Record<string, string> = {
  "harm-upfront-payment-fee": "Asks for an upfront payment or fee",
  "harm-pii-before-hire": "Wants sensitive personal info before hiring",
  "harm-payment-handling-mule": "Money-mule / payment-handling duties",
  "harm-fake-check-overpayment": "Send-a-check / overpayment scheme",
  "harm-task-deposit-crypto": "Deposit-to-unlock task / crypto scheme",
  "harm-reshipping-parcel": "Reshipping / parcel-handling scheme",
  "harm-brand-impersonation-freemail": "Big brand contacting via free webmail",
  "weak-offplatform-messaging": "Pushes contact to Telegram/WhatsApp",
  "weak-freemail-apply-contact": "Apply by a free webmail address",
  "weak-unrealistic-pay-period": "Implausibly high short-term pay",
  "weak-no-experience-high-pay": "No experience needed + outsized pay",
  "weak-mlm-recruiting-income": "MLM / recruit-for-income language",
  "weak-thin-vague-description": "Thin or duties-absent description",
  "weak-generic-placeholder-company": "Missing or placeholder company",
  "weak-apply-url-suspicious-host": "Shortened / non-standard apply link",
  "weak-title-shouting-spam": "Shouting / spammy title",
  "weak-urgency-no-interview": "Instant-hire / no-interview pressure",
};

// Languages offered in the search filter (ISO 639-1 -> display name). The detector
// recognizes more, but these are the ones surfaced as filter chips.
export const LANG_LABELS: Record<string, string> = {
  en: "English",
  pt: "Portuguese",
  es: "Spanish",
  fr: "French",
  de: "German",
  nl: "Dutch",
  it: "Italian",
  pl: "Polish",
};

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

export const APP_STATUSES = [
  "saved",
  "applied",
  "screen",
  "interview",
  "offer",
  "accepted",
  "rejected",
] as const;
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
  {
    name: "LinkedIn REACH",
    org: "LinkedIn",
    url: "https://careers.linkedin.com/reach",
    tag: "Paid apprenticeship",
  },
  {
    name: "Amazon Technical Academy",
    org: "Amazon",
    url: "https://www.amazon.jobs/content/en/teams/amazon-web-services/technical-apprenticeships",
    tag: "Paid apprenticeship",
  },
  {
    name: "Microsoft LEAP",
    org: "Microsoft",
    url: "https://leap.microsoft.com/",
    tag: "Paid apprenticeship",
  },
  { name: "Multiverse", org: "Multiverse", url: "https://www.multiverse.io/", tag: "Apprenticeships" },
  { name: "Year Up", org: "Year Up United", url: "https://www.yearup.org/", tag: "Free + stipend" },
  { name: "Per Scholas", org: "Per Scholas", url: "https://perscholas.org/", tag: "Free + stipend" },
  {
    name: "apprenticeship.gov",
    org: "US DOL",
    url: "https://www.apprenticeship.gov/",
    tag: "Registered apprenticeships",
  },
];

export const SOURCE_KINDS = [
  "greenhouse",
  "lever",
  "ashby",
  "arbeitnow",
  "remoteok",
  "remotive",
  "jobicy",
  "himalayas",
  "weworkremotely",
  "workingnomads",
  "workable",
  "recruitee",
  "smartrecruiters",
  "jsonld",
  "rss",
  "adzuna",
  "themuse",
  "findwork",
  "reed",
  "jooble",
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
  remoteok: "RemoteOK",
  remotive: "Remotive",
  jobicy: "Jobicy",
  himalayas: "Himalayas",
  weworkremotely: "We Work Remotely",
  workingnomads: "Working Nomads",
  workable: "Workable",
  recruitee: "Recruitee",
  smartrecruiters: "SmartRecruiters",
  jsonld: "Career page",
  rss: "RSS feed",
  adzuna: "Adzuna",
  themuse: "The Muse",
  findwork: "Findwork",
  reed: "Reed",
  jooble: "Jooble",
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
  rss: [
    { key: "url", label: "Feed URL (RSS or Atom)", placeholder: "https://example.com/jobs.rss" },
    { key: "company", label: "Company (optional)", placeholder: "overrides the feed author" },
  ],
  weworkremotely: [{ key: "category", label: "Category (optional)", placeholder: "remote-programming-jobs" }],
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
  themuse: [
    { key: "api_key", label: "API key (optional, raises rate limit)" },
    { key: "category", label: "Category (optional)", placeholder: "Software Engineering" },
    { key: "location", label: "Location (optional)", placeholder: "Lisbon, Portugal" },
    { key: "level", label: "Level (optional)", placeholder: "Senior Level" },
    { key: "company", label: "Company (optional)" },
  ],
  findwork: [
    { key: "api_key", label: "API key", placeholder: "from findwork.dev/developers" },
    { key: "search", label: "Keywords (optional)", placeholder: "react developer" },
    { key: "location", label: "Location (optional)", placeholder: "Lisbon" },
    { key: "remote", label: "Remote only? (optional)", placeholder: "true" },
    { key: "auth_scheme", label: "Auth scheme (optional)", placeholder: "Token" },
  ],
  reed: [
    { key: "api_key", label: "API key", placeholder: "from reed.co.uk/developers" },
    { key: "keywords", label: "Keywords (optional)", placeholder: ".net developer" },
    { key: "location", label: "Location (optional)", placeholder: "London" },
  ],
  jooble: [
    { key: "api_key", label: "API key", placeholder: "from jooble.org/api/about" },
    { key: "keywords", label: "Keywords (optional)", placeholder: "software engineer" },
    { key: "location", label: "Location (optional)", placeholder: "Porto" },
  ],
  arbeitnow: [],
  remoteok: [],
  remotive: [],
  jobicy: [],
  himalayas: [],
  workingnomads: [],
  workable: [{ key: "account", label: "Account", placeholder: "your-company (apply.workable.com/...)" }],
  recruitee: [{ key: "company", label: "Company", placeholder: "your-company (.recruitee.com)" }],
  smartrecruiters: [{ key: "company", label: "Company ID", placeholder: "CompanyName" }],
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

// `inPrice`/`outPrice` are USD per million tokens, for the in-app cost estimate.
// Approximate published Claude pricing; the real bill lives in the Anthropic console.
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

export const ASSISTANT_MODELS: {
  id: string;
  label: string;
  note: string;
  inPrice: number;
  outPrice: number;
}[] = [
  {
    id: "claude-opus-4-8",
    label: "Claude Opus 4.8",
    note: "most capable, slower",
    inPrice: 15,
    outPrice: 75,
  },
  { id: "claude-sonnet-5", label: "Claude Sonnet 5", note: "balanced, near-Opus", inPrice: 3, outPrice: 15 },
  { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6", note: "balanced (older)", inPrice: 3, outPrice: 15 },
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

// Per-profile job-search preferences: the saved search filters that travel with a profile.
export interface JobPrefs {
  workplaces: string[];
  levels: string[];
  selectedKinds: string[];
  remoteEligible: boolean;
  relocationOnly: boolean;
  postedWithin: number | null;
  minSalary: number | null;
  langs: string[];
  hideScams: boolean;
  sortMode: "default" | "match";
  minMatch: number | null;
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
