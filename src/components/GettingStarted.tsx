import { Bot, ClipboardList, GraduationCap, Search, X } from "lucide-react";

type View = "search" | "tracker" | "study" | "assistant";

const AREAS: { view: View; icon: typeof Bot; title: string; blurb: string }[] = [
  {
    view: "search",
    icon: Search,
    title: "Search",
    blurb: "Aggregate jobs from public boards into a local database. Filter by workplace, level, and what's workable from your location; track any listing with one click.",
  },
  {
    view: "tracker",
    icon: ClipboardList,
    title: "Tracker",
    blurb: "A pipeline board for applications and training programs - deadlines, next actions, and AI interview prep per role.",
  },
  {
    view: "study",
    icon: GraduationCap,
    title: "Study",
    blurb: "AI study plans for skills you're building, plus a Mock interview that asks tailored questions and scores your answers on content and delivery.",
  },
  {
    view: "assistant",
    icon: Bot,
    title: "Assistant",
    blurb: "Your live interview copilot: real-time transcription, streaming auto-answers, screen/page capture, your story bank, an application kit, and saved-session review.",
  },
];

// First-run guide to the (now sizable) feature set. Re-openable from the top-bar "?".
export function GettingStarted({ onClose, onGo }: { onClose: () => void; onGo: (v: View) => void }) {
  return (
    <div className="intro-overlay" onClick={onClose}>
      <div className="intro-card" onClick={(e) => e.stopPropagation()}>
        <button className="icon-btn intro-close" title="Close" onClick={onClose}>
          <X size={18} />
        </button>
        <h2>Welcome to JobRadar</h2>
        <p className="muted">
          A local-first copilot for the whole journey - find roles, apply, ace the interview, and study to
          improve. Most of it runs on your machine; cloud is used only when you opt in.
        </p>

        <div className="intro-grid">
          {AREAS.map((a) => (
            <button key={a.view} className="intro-area" onClick={() => onGo(a.view)}>
              <span className="intro-area-head">
                <a.icon size={16} /> {a.title}
              </span>
              <span className="muted sm">{a.blurb}</span>
            </button>
          ))}
        </div>

        <div className="intro-tips">
          <strong>Tips</strong>
          <ul>
            <li>
              In a call: open <em>Assistant</em>, hit <em>Listen to call</em>, set your language, and answers
              stream in live as questions are asked.
            </li>
            <li>
              Set up the <em>Interview context</em> and <em>Your stories</em> once - every answer, the
              application kit, and mock-interview feedback ground in them.
            </li>
            <li>
              Can't copy a locked page? Press <span className="kbd">Alt+Shift+R</span> to read it via
              accessibility. Toggle the <span className="kbd">eye</span> icon to hide the window from screen
              shares.
            </li>
          </ul>
        </div>

        <div className="intro-foot">
          <span className="muted sm">Reopen this anytime from the ? in the top bar.</span>
          <button className="btn-open" onClick={onClose}>
            Got it
          </button>
        </div>
      </div>
    </div>
  );
}
