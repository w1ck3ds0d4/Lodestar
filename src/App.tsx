import { useCallback, useEffect, useState } from "react";
import { Bot, Eye, EyeOff, HelpCircle, Keyboard, Radar, Settings as SettingsIcon, UserRound } from "lucide-react";
import * as api from "./api";
import { AssistantView } from "./components/AssistantView";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { GettingStarted } from "./components/GettingStarted";
import { ProfileHub } from "./components/ProfileHub";
import { Shortcuts } from "./components/Shortcuts";
import { ResizeHandles, WindowControls } from "./components/WindowChrome";

// The main window is borderless (decorations off), so it draws its own title bar (the top
// bar) and window controls. In a plain browser (Vite preview) there is no Tauri window, so
// skip the chrome there.
const HAS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type View = "assistant" | "settings";

export default function App() {
  const [view, setView] = useState<View>("assistant");
  const [capHidden, setCapHidden] = useState(true);
  const [profileOpen, setProfileOpen] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showIntro, setShowIntro] = useState(() => localStorage.getItem("jr.introSeen") !== "1");
  const dismissIntro = useCallback(() => {
    localStorage.setItem("jr.introSeen", "1");
    setShowIntro(false);
  }, []);

  // Re-assert stealth (out of Alt+Tab, filed under Task Manager "Background processes").
  useEffect(() => {
    void api.applyWindowStealth().catch(() => {});
  }, []);

  // Fade out and remove the launch splash (static markup in index.html). The Assistant owns
  // its own loading states, so the shell is interactive as soon as it mounts.
  useEffect(() => {
    const el = document.getElementById("jr-splash");
    if (!el) return;
    el.classList.add("jr-hide");
    const t = window.setTimeout(() => el.remove(), 400);
    return () => window.clearTimeout(t);
  }, []);

  const toggleCapture = useCallback(async () => {
    const next = !capHidden;
    await api.setScreenPrivacy(next);
    setCapHidden(next);
  }, [capHidden]);

  return (
    <div className="app">
      {HAS_TAURI && <ResizeHandles />}
      <header className="topbar" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <Radar size={22} className="brand-icon" />
          <span>Lodestar</span>
        </div>
        <nav className="nav">
          <button
            className={"nav-btn" + (view === "assistant" ? " active" : "")}
            onClick={() => setView("assistant")}
            title="Assistant"
          >
            <Bot size={15} /> <span className="nav-label">Assistant</span>
          </button>
          <button
            className={"nav-btn" + (view === "settings" ? " active" : "")}
            onClick={() => setView("settings")}
            title="Settings"
          >
            <SettingsIcon size={15} /> <span className="nav-label">Settings</span>
          </button>
        </nav>
        <div className="topbar-actions">
          <button
            className={"icon-btn" + (capHidden ? " active" : "")}
            title={
              capHidden
                ? "Hidden from screen capture - click to show in shares"
                : "Visible to screen capture - click to hide"
            }
            onClick={toggleCapture}
          >
            {capHidden ? <EyeOff size={18} /> : <Eye size={18} />}
          </button>
          <button className="icon-btn" title="Your profiles" onClick={() => setProfileOpen(true)}>
            <UserRound size={18} />
          </button>
          <button className="icon-btn" title="Getting started" onClick={() => setShowIntro(true)}>
            <HelpCircle size={18} />
          </button>
          <button
            className="icon-btn"
            title="Keyboard shortcuts"
            onClick={() => setShowShortcuts(true)}
          >
            <Keyboard size={18} />
          </button>
          {HAS_TAURI && <WindowControls />}
        </div>
      </header>

      <ProfileHub open={profileOpen} onClose={() => setProfileOpen(false)} />

      {/* AssistantView owns the live session (listener, transcript, segments), so it is
          NEVER unmounted when switching tabs - swapping it out mid-call would throw away the
          transcript. It renders the Settings page itself from that same state. */}
      <main className="results">
        <ErrorBoundary key="assistant" label="Assistant">
          <AssistantView view={view} />
        </ErrorBoundary>
      </main>

      {showIntro && <GettingStarted onClose={dismissIntro} />}

      {showShortcuts && <Shortcuts onClose={() => setShowShortcuts(false)} />}
    </div>
  );
}
