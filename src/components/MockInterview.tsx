import { useState } from "react";
import { Mic, RefreshCw, Sparkles, Volume2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import * as api from "../api";

const FILLERS = [
  "um",
  "uh",
  "er",
  "like",
  "you know",
  "basically",
  "actually",
  "literally",
  "kind of",
  "sort of",
  "i mean",
];

// Count filler words/phrases in an answer (case-insensitive). Single words match on
// word boundaries; phrases match as substrings.
function fillerCount(text: string): number {
  const lower = " " + text.toLowerCase().replace(/[^a-z\s]/g, " ") + " ";
  let n = 0;
  for (const f of FILLERS) {
    if (f.includes(" ")) {
      n += lower.split(f).length - 1;
    } else {
      const m = lower.match(new RegExp(`\\s${f}\\s`, "g"));
      n += m ? m.length : 0;
    }
  }
  return n;
}

// AI mock interviewer: asks role-tailored questions, scores your answer on content
// and delivery, and shows a model answer. Grounded in your saved profile + stories.
export function MockInterview() {
  const [question, setQuestion] = useState("");
  const [asked, setAsked] = useState<string[]>([]);
  const [answer, setAnswer] = useState("");
  const [feedback, setFeedback] = useState("");
  const [busy, setBusy] = useState<"q" | "fb" | "">("");
  const [err, setErr] = useState("");
  const [speak, setSpeak] = useState(false);

  const words = answer.trim() ? answer.trim().split(/\s+/).length : 0;
  const fillers = fillerCount(answer);

  const say = (t: string) => {
    if (!speak) return;
    try {
      window.speechSynthesis.cancel();
      window.speechSynthesis.speak(new SpeechSynthesisUtterance(t));
    } catch {
      /* speechSynthesis not available */
    }
  };

  const nextQuestion = async () => {
    setBusy("q");
    setErr("");
    setFeedback("");
    setAnswer("");
    try {
      const q = await api.practiceQuestion(asked);
      setQuestion(q);
      setAsked((a) => [...a, q]);
      say(q);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy("");
    }
  };

  const getFeedback = async () => {
    if (!answer.trim()) {
      setErr("Type or paste your answer first.");
      return;
    }
    setBusy("fb");
    setErr("");
    try {
      const stats = `(Delivery stats: ${words} words, ${fillers} filler word${fillers === 1 ? "" : "s"})`;
      setFeedback(await api.practiceFeedback(question, `${stats}\n\n${answer}`));
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="add-source mock">
      <h3>
        <Mic size={16} /> Mock interview
      </h3>
      <p className="muted sm">
        Practice with an AI interviewer tailored to your role and company. It scores your answer on content
        and delivery and shows a model answer. Uses your saved profile + stories.
      </p>

      <div className="mock-bar">
        <button className="btn-open" onClick={() => void nextQuestion()} disabled={busy === "q"}>
          <RefreshCw size={14} className={busy === "q" ? "spin" : ""} />
          {asked.length === 0 ? "Start interview" : "Next question"}
        </button>
        <label className="switch sm" title="Read each question aloud (browser text-to-speech)">
          <input type="checkbox" checked={speak} onChange={(e) => setSpeak(e.target.checked)} />
          <Volume2 size={14} /> Speak
        </label>
      </div>

      {question && (
        <>
          <div className="mock-question">{question}</div>
          <textarea
            rows={5}
            placeholder="Type (or paste a transcript of) your answer, then get feedback..."
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
          />
          <div className="mock-foot">
            <span className="muted sm">
              {words} words · {fillers} filler{fillers === 1 ? "" : "s"}
            </span>
            <button className="btn-sync" onClick={() => void getFeedback()} disabled={busy === "fb" || !answer.trim()}>
              <Sparkles size={15} /> {busy === "fb" ? "Scoring..." : "Get feedback"}
            </button>
          </div>
        </>
      )}

      {err && <div className="assistant-error">{err}</div>}
      {feedback && (
        <div className="assistant-answer mock-feedback">
          <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
            {feedback}
          </ReactMarkdown>
        </div>
      )}
    </div>
  );
}
