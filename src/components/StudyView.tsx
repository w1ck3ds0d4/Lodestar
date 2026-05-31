import { useCallback, useEffect, useState } from "react";
import { Plus, Sparkles, Trash2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import * as api from "../api";
import type { Skill } from "../types";

function PlanMarkdown({ text }: { text: string }) {
  return (
    <div className="assistant-answer">
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
        {text}
      </ReactMarkdown>
    </div>
  );
}

export function StudyView() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const load = useCallback(async () => setSkills(await api.listSkills()), []);
  useEffect(() => {
    void load();
  }, [load]);

  const [name, setName] = useState("");
  const [goal, setGoal] = useState("");
  const add = async () => {
    if (!name.trim()) return;
    await api.addSkill(name.trim(), goal.trim());
    setName("");
    setGoal("");
    load();
  };

  return (
    <div className="study">
      <div className="add-source">
        <h3>Add a skill to study</h3>
        <div className="add-row">
          <input placeholder="Skill (e.g. Rust, System design)" value={name} onChange={(e) => setName(e.target.value)} />
          <input placeholder="Goal (optional)" value={goal} onChange={(e) => setGoal(e.target.value)} />
          <button className="btn-open" onClick={add}><Plus size={14} /> Add</button>
        </div>
      </div>
      <div className="skill-list">
        {skills.length === 0 && (
          <p className="muted">No skills yet. Add one above - the assistant can also generate a study plan that fills this in.</p>
        )}
        {skills.map((s) => (
          <SkillRow key={s.id} skill={s} onChanged={load} />
        ))}
      </div>
    </div>
  );
}

function SkillRow({ skill, onChanged }: { skill: Skill; onChanged: () => void }) {
  const [level, setLevel] = useState(skill.level);
  const [hoursToLog, setHoursToLog] = useState("1");
  const [planning, setPlanning] = useState(false);
  const [planError, setPlanError] = useState("");

  const commitLevel = async () => {
    await api.updateSkill(skill.id, skill.name, skill.goal, level, skill.notes);
  };
  const log = async () => {
    const h = parseFloat(hoursToLog) || 0;
    if (h <= 0) return;
    await api.logStudy(skill.id, h);
    onChanged();
  };
  const del = async () => {
    await api.removeSkill(skill.id);
    onChanged();
  };
  const generatePlan = async () => {
    setPlanning(true);
    setPlanError("");
    try {
      const plan = await api.studyPlan(skill.name, skill.goal);
      await api.updateSkill(skill.id, skill.name, skill.goal, level, plan);
      onChanged();
    } catch (e) {
      setPlanError(String(e));
    } finally {
      setPlanning(false);
    }
  };

  return (
    <div className="skill-row">
      <div className="skill-head">
        <div>
          <div className="skill-name">{skill.name}</div>
          {skill.goal && <div className="muted sm">{skill.goal}</div>}
        </div>
        <button className="icon-btn danger sm" title="Remove" onClick={del}><Trash2 size={14} /></button>
      </div>
      <div className="skill-level">
        <span className="muted sm">Level {level}%</span>
        <input
          type="range"
          min={0}
          max={100}
          value={level}
          onChange={(e) => setLevel(Number(e.target.value))}
          onMouseUp={commitLevel}
          onTouchEnd={commitLevel}
        />
      </div>
      <div className="progress-bar"><div className="progress-fill" style={{ width: `${level}%` }} /></div>
      <div className="skill-hours">
        <span className="muted sm">{skill.hours.toFixed(1)} h logged</span>
        <input type="number" min={0} step={0.5} value={hoursToLog} onChange={(e) => setHoursToLog(e.target.value)} />
        <button className="btn-open" onClick={log}>Log hours</button>
        <button className="btn-sync" onClick={generatePlan} disabled={planning}>
          <Sparkles size={14} /> {planning ? "Generating…" : skill.notes ? "Regenerate plan" : "Study plan"}
        </button>
      </div>
      {planError && <div className="assistant-error">{planError}</div>}
      {skill.notes && (
        <details className="study-plan" open>
          <summary className="muted sm">Study plan</summary>
          <PlanMarkdown text={skill.notes} />
        </details>
      )}
    </div>
  );
}
