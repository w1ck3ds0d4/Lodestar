import { useCallback, useEffect, useState } from "react";
import { Layers, Sparkles, Trash2 } from "lucide-react";
import * as api from "../api";
import type { Card } from "../types";

// Spaced-repetition flashcards (Leitner). Generate cards from study material - a
// session review, weak areas, or a topic - then review the ones that are due.
export function Flashcards() {
  const [cards, setCards] = useState<Card[]>([]);
  const [source, setSource] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState("");
  const [revealed, setRevealed] = useState(false);

  const load = useCallback(async () => {
    try {
      setCards(await api.listCards());
    } catch {
      /* ignore */
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const due = cards.filter((c) => c.dueAt <= Date.now());
  const current = due[0];

  const generate = async () => {
    if (!source.trim()) return;
    setBusy(true);
    setMsg("");
    try {
      const n = await api.generateCards(source);
      setSource("");
      setMsg(`Added ${n} card${n === 1 ? "" : "s"}.`);
      window.setTimeout(() => setMsg(""), 3000);
      await load();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };

  const rate = async (good: boolean) => {
    if (!current) return;
    setRevealed(false);
    await api.reviewCard(current.id, good);
    await load();
  };

  const del = async (id: number) => {
    await api.removeCard(id);
    await load();
  };

  return (
    <div className="add-source cards">
      <h3>
        <Layers size={16} /> Flashcards
      </h3>
      <p className="muted sm">
        Spaced repetition for your weak areas. Paste a session review, weak spots, or a topic, generate cards,
        then review the ones due.
      </p>
      <textarea
        rows={3}
        placeholder="Paste material to make cards from (a session review, weak areas, or a topic)..."
        value={source}
        onChange={(e) => setSource(e.target.value)}
      />
      <div className="cards-bar">
        <button className="btn-sync" onClick={() => void generate()} disabled={busy || !source.trim()}>
          <Sparkles size={15} /> {busy ? "Generating..." : "Generate cards"}
        </button>
        <span className="muted sm">
          {due.length} due · {cards.length} total
        </span>
      </div>
      {msg && <div className="muted sm">{msg}</div>}

      {current ? (
        <div className="card-review">
          <div className="card-face">{current.front}</div>
          {revealed ? (
            <>
              <div className="card-face back">{current.back}</div>
              <div className="cards-bar">
                <button className="btn-open" onClick={() => void rate(false)}>
                  Again
                </button>
                <button className="btn-sync" onClick={() => void rate(true)}>
                  Good
                </button>
              </div>
            </>
          ) : (
            <button className="btn-open" onClick={() => setRevealed(true)}>
              Show answer
            </button>
          )}
        </div>
      ) : cards.length > 0 ? (
        <div className="muted sm">All caught up - nothing due right now.</div>
      ) : null}

      {cards.length > 0 && (
        <details className="cards-manage">
          <summary className="muted sm">Manage {cards.length} cards</summary>
          {cards.map((c) => (
            <div key={c.id} className="card-row">
              <span className="card-row-front">{c.front}</span>
              <button className="icon-btn danger xs" title="Delete card" onClick={() => void del(c.id)}>
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </details>
      )}
    </div>
  );
}
