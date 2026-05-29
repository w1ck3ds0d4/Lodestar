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
