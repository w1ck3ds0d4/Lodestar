import { invoke } from "@tauri-apps/api/core";
import type { Alert, Job, Source, Stats } from "./types";

export const searchJobs = (
  query: string,
  workplaces: string[],
  levels: string[],
  kinds: string[],
  limit = 200,
) => invoke<Job[]>("search_jobs", { query, workplaces, levels, kinds, limit });

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
