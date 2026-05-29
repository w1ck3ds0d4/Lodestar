import { useState } from "react";
import { Bell, Plus, Trash2 } from "lucide-react";
import type { Alert } from "../types";
import * as api from "../api";

export function AlertsPanel({
  alerts,
  presetQuery,
  onChanged,
  onClose,
}: {
  alerts: Alert[];
  presetQuery: string;
  onChanged: () => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState(presetQuery);
  const [remoteOnly, setRemoteOnly] = useState(false);

  const add = async () => {
    await api.addAlert(query.trim(), remoteOnly);
    onChanged();
  };

  const del = async (id: number) => {
    await api.removeAlert(id);
    onChanged();
  };

  return (
    <div className="drawer-overlay" onClick={onClose}>
      <div className="drawer" onClick={(e) => e.stopPropagation()}>
        <div className="drawer-head">
          <h2>Alerts</h2>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>
        <div className="drawer-body">
          <p className="muted">Get a desktop notification when new jobs matching a saved search arrive.</p>

          <div className="add-source">
            <h3>New alert</h3>
            <input
              placeholder="Search terms (e.g. rust backend)"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <label className="switch">
              <input type="checkbox" checked={remoteOnly} onChange={(e) => setRemoteOnly(e.target.checked)} />
              <span>Remote only</span>
            </label>
            <button className="btn-open" onClick={add}><Plus size={14} /> Create alert</button>
          </div>

          <div className="src-list">
            {alerts.length === 0 && <p className="muted">No alerts yet.</p>}
            {alerts.map((a) => (
              <div className="src-row" key={a.id}>
                <div className="src-row-top">
                  <Bell size={16} />
                  <span className="src-label-static">{a.query.trim() || "(any job)"}</span>
                  {a.remoteOnly && <span className="badge badge-remote">Remote</span>}
                  <div className="spacer" />
                  <button className="icon-btn danger" onClick={() => del(a.id)}><Trash2 size={16} /></button>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
