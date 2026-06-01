import { useCallback, useEffect, useState } from "react";
import { Plus, Sparkles, Trash2 } from "lucide-react";
import * as api from "../api";
import type { Story } from "../types";

// The interview story bank: the user saves real STAR stories once, and behavioral
// answers draw on the most relevant one (grounded via the answer system prompt).
export function StoriesPanel() {
  const [stories, setStories] = useState<Story[]>([]);
  const [draftNotes, setDraftNotes] = useState("");
  const [drafting, setDrafting] = useState(false);
  const [err, setErr] = useState("");

  const load = useCallback(async () => {
    try {
      setStories(await api.listStories());
    } catch (e) {
      setErr(String(e));
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const addBlank = async () => {
    await api.addStory("New story", "", "");
    await load();
  };

  const draft = async () => {
    if (!draftNotes.trim()) return;
    setDrafting(true);
    setErr("");
    try {
      const text = await api.draftStory(draftNotes);
      const title = draftNotes.trim().split(/\s+/).slice(0, 6).join(" ");
      await api.addStory(title || "Drafted story", text, "");
      setDraftNotes("");
      await load();
    } catch (e) {
      setErr(String(e));
    } finally {
      setDrafting(false);
    }
  };

  return (
    <div className="stories">
      <p className="muted sm">
        Save your real STAR stories once. Behavioral answers will draw on the most relevant one and weave in
        your details, instead of generic filler.
      </p>
      <div className="story-draft">
        <textarea
          rows={2}
          placeholder="Rough notes about an experience - the AI drafts a STAR story you can edit and save..."
          value={draftNotes}
          onChange={(e) => setDraftNotes(e.target.value)}
        />
        <button className="btn-sync" onClick={() => void draft()} disabled={drafting || !draftNotes.trim()}>
          <Sparkles size={15} /> {drafting ? "Drafting..." : "Draft a STAR story"}
        </button>
      </div>
      {err && <div className="assistant-error">{err}</div>}
      {stories.map((s) => (
        <StoryCard key={s.id} story={s} onChanged={load} />
      ))}
      <button className="btn-open" onClick={() => void addBlank()}>
        <Plus size={14} /> Add a blank story
      </button>
    </div>
  );
}

function StoryCard({ story, onChanged }: { story: Story; onChanged: () => void }) {
  const [title, setTitle] = useState(story.title);
  const [tags, setTags] = useState(story.tags);
  const [content, setContent] = useState(story.content);
  const [saved, setSaved] = useState(false);
  const dirty = title !== story.title || tags !== story.tags || content !== story.content;

  const save = async () => {
    await api.updateStory(story.id, title, content, tags);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
    onChanged();
  };
  const del = async () => {
    await api.removeStory(story.id);
    onChanged();
  };

  return (
    <div className="story-card">
      <div className="story-head">
        <input
          className="story-title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Story title"
        />
        <button className="icon-btn danger sm" title="Delete story" onClick={() => void del()}>
          <Trash2 size={14} />
        </button>
      </div>
      <input
        className="story-tags"
        value={tags}
        onChange={(e) => setTags(e.target.value)}
        placeholder="Tags (e.g. leadership, conflict, scaling)"
      />
      <textarea
        rows={5}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        placeholder="Situation / Task / Action / Result..."
      />
      <button className="btn-open sm" onClick={() => void save()} disabled={!dirty}>
        {saved ? "Saved ✓" : "Save"}
      </button>
    </div>
  );
}
