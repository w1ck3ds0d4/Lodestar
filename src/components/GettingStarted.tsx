import { Bot, X } from "lucide-react";

const AREAS: { key: string; icon: typeof Bot; title: string; blurb: string }[] = [
  {
    key: "assistant",
    icon: Bot,
    title: "Assistant",
    blurb:
      "Your live interview copilot: real-time transcription, streaming auto-answers, screen/page capture, your story bank, an application kit, and saved-session review.",
  },
];

// First-run guide to the feature set. Re-openable from the top-bar "?".
export function GettingStarted({ onClose }: { onClose: () => void }) {
  return (
    <div className="intro-overlay" onClick={onClose}>
      <div className="intro-card" onClick={(e) => e.stopPropagation()}>
        <button className="icon-btn intro-close" title="Close" onClick={onClose}>
          <X size={18} />
        </button>
        <h2>Welcome to Lodestar</h2>
        <p className="muted">
          A local-first interview copilot: hear the question, get the answer, review it afterwards. Most
          of it runs on your machine; cloud is used only when you opt in.
        </p>

        <div className="intro-grid">
          {AREAS.map((a) => (
            <div key={a.key} className="intro-area">
              <span className="intro-area-head">
                <a.icon size={16} /> {a.title}
              </span>
              <span className="muted sm">{a.blurb}</span>
            </div>
          ))}
        </div>

        <div className="intro-tips">
          <strong>Tips</strong>
          <ul>
            <li>
              In a call: hit <em>Listen to call</em>, set your language, and answers stream in live as
              questions are asked.
            </li>
            <li>
              Set up the <em>Interview context</em> and <em>Your stories</em> once - every answer and the
              application kit ground in them.
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
