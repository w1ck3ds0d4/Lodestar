import { invoke } from "@tauri-apps/api/core";
import type { Alert, AssistantSettings, AudioDevices, Job, Skill, Source, Stats, Tracked } from "./types";

export const searchJobs = (
  query: string,
  workplaces: string[],
  levels: string[],
  kinds: string[],
  relocationOnly = false,
  remoteEligible = false,
  limit = 200,
) =>
  invoke<Job[]>("search_jobs", {
    query,
    workplaces,
    levels,
    kinds,
    relocationOnly,
    remoteEligible,
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

// --- Screen privacy (exclude window from screen capture / sharing) ---
export const setScreenPrivacy = (enabled: boolean) => invoke<void>("set_screen_privacy", { enabled });

// --- Assistant (Claude API) ---
export const getAssistantSettings = () => invoke<AssistantSettings>("get_assistant_settings");
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });
export const setAssistantModel = (model: string) => invoke<void>("set_assistant_model", { model });
export const askAssistant = (question: string, context: string | null) =>
  invoke<string>("ask_assistant", { question, context });
export const autoAnswer = (transcript: string, latest: string) =>
  invoke<string>("auto_answer", { transcript, latest });

// --- Live listening (system-audio capture -> Deepgram) ---
export const setDeepgramKey = (key: string) => invoke<void>("set_deepgram_key", { key });
export const setSttEngine = (engine: string) => invoke<void>("set_stt_engine", { engine });
export const setCaptureSource = (source: string) => invoke<void>("set_capture_source", { source });
export const listAudioDevices = () => invoke<AudioDevices>("list_audio_devices");
export const setInputDevice = (name: string) => invoke<void>("set_input_device", { name });
export const setOutputDevice = (name: string) => invoke<void>("set_output_device", { name });
export const startListening = () => invoke<void>("start_listening");
export const stopListening = () => invoke<void>("stop_listening");
