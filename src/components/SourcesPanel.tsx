import { useState } from "react";
import { Check, Play, Plus, Trash2 } from "lucide-react";
import type { Source } from "../types";
import { CONFIG_FIELDS, KIND_LABELS, SOURCE_KINDS } from "../types";
import * as api from "../api";

const fieldsFor = (kind: string) => CONFIG_FIELDS[kind] ?? [];

function SourceRow({ source, onChanged }: { source: Source; onChanged: () => void }) {
  const [label, setLabel] = useState(source.label);
  const [enabled, setEnabled] = useState(source.enabled);
  const [config, setConfig] = useState<Record<string, string>>({ ...source.config });
  const [intervalMin, setIntervalMin] = useState(Math.max(1, Math.round(source.minIntervalSecs / 60)));
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  // Inline result of the last "Test" run: count on success, error string on failure.
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string } | null>(null);

  const ok = source.lastStatus?.startsWith("ok");

  const save = async () => {
    setSaving(true);
    try {
      await api.updateSource(source.id, label, enabled, config, intervalMin * 60);
      onChanged();
    } finally {
      setSaving(false);
    }
  };

  // Save first (so a freshly typed key/config is used), then fetch this one source now.
  const test = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      await api.updateSource(source.id, label, enabled, config, intervalMin * 60);
      const n = await api.runSource(source.id);
      setTestResult({ ok: true, text: `${n} job${n === 1 ? "" : "s"}` });
      onChanged();
    } catch (e) {
      setTestResult({ ok: false, text: String(e) });
      onChanged();
    } finally {
      setTesting(false);
    }
  };

  const del = async () => {
    if (!confirm(`Delete source "${source.label}"? Its jobs will be removed.`)) return;
    await api.removeSource(source.id);
    onChanged();
  };

  return (
    <div className="src-row">
      <div className="src-row-top">
        <label className="switch">
          <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
          <span>{enabled ? "On" : "Off"}</span>
        </label>
        <span className="badge badge-source">{KIND_LABELS[source.kind] ?? source.kind}</span>
        <input className="src-label" value={label} onChange={(e) => setLabel(e.target.value)} />
        <button className="icon-btn danger" title="Delete" onClick={del}>
          <Trash2 size={16} />
        </button>
      </div>

      {fieldsFor(source.kind).length > 0 && (
        <div className="src-fields">
          {fieldsFor(source.kind).map((f) => (
            <label key={f.key} className="field">
              <span>{f.label}</span>
              <input
                value={config[f.key] ?? ""}
                placeholder={f.placeholder}
                onChange={(e) => setConfig((c) => ({ ...c, [f.key]: e.target.value }))}
              />
            </label>
          ))}
        </div>
      )}

      <div className="src-row-bottom">
        <label className="field inline">
          <span>Every</span>
          <input
            type="number"
            min={1}
            value={intervalMin}
            onChange={(e) => setIntervalMin(Math.max(1, Number(e.target.value) || 1))}
          />
          <span>min</span>
        </label>
        {testResult ? (
          <span className={"src-status " + (testResult.ok ? "ok" : "err")} title={testResult.text}>
            {testResult.text}
          </span>
        ) : (
          source.lastStatus && (
            <span className={"src-status " + (ok ? "ok" : "err")} title={source.lastStatus}>
              {ok ? `${source.lastCount ?? 0} jobs` : source.lastStatus}
            </span>
          )
        )}
        <div className="spacer" />
        <button
          className="btn-ghost"
          onClick={test}
          disabled={testing || saving}
          title="Save and fetch this source now"
        >
          <Play size={14} /> {testing ? "Testing..." : "Test"}
        </button>
        <button className="btn-open" onClick={save} disabled={saving || testing}>
          <Check size={14} /> Save
        </button>
      </div>
    </div>
  );
}

export function SourcesPanel({
  sources,
  onChanged,
  onClose,
}: {
  sources: Source[];
  onChanged: () => void;
  onClose: () => void;
}) {
  const [newKind, setNewKind] = useState<string>("greenhouse");
  const [newLabel, setNewLabel] = useState("");
  const [newConfig, setNewConfig] = useState<Record<string, string>>({});

  const add = async () => {
    const label = newLabel.trim() || KIND_LABELS[newKind] || newKind;
    await api.addSource(newKind, label, newConfig, 21600);
    setNewLabel("");
    setNewConfig({});
    onChanged();
  };

  return (
    <div className="drawer-overlay" onClick={onClose}>
      <div className="drawer" onClick={(e) => e.stopPropagation()}>
        <div className="drawer-head">
          <h2>Sources</h2>
          <button className="icon-btn" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="drawer-body">
          <div className="add-source">
            <h3>Add a source</h3>
            <div className="add-row">
              <select
                value={newKind}
                onChange={(e) => {
                  setNewKind(e.target.value);
                  setNewConfig({});
                }}
              >
                {SOURCE_KINDS.map((k) => (
                  <option key={k} value={k}>
                    {KIND_LABELS[k]}
                  </option>
                ))}
              </select>
              <input
                placeholder="Label (optional)"
                value={newLabel}
                onChange={(e) => setNewLabel(e.target.value)}
              />
            </div>
            {fieldsFor(newKind).length > 0 && (
              <div className="src-fields">
                {fieldsFor(newKind).map((f) => (
                  <label key={f.key} className="field">
                    <span>{f.label}</span>
                    <input
                      value={newConfig[f.key] ?? ""}
                      placeholder={f.placeholder}
                      onChange={(e) => setNewConfig((c) => ({ ...c, [f.key]: e.target.value }))}
                    />
                  </label>
                ))}
              </div>
            )}
            <button className="btn-open" onClick={add}>
              <Plus size={14} /> Add source
            </button>
          </div>

          <div className="src-list">
            {sources.map((s) => (
              <SourceRow key={s.id} source={s} onChanged={onChanged} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
