import { useEffect } from "react";
import { emit, type UnlistenFn } from "./tauriEvent";
import { availableMonitors, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import type { CaptureCommand } from "../types";

// Send a command from a capture overlay window back to the main window.
export const sendCaptureCommand = (c: CaptureCommand) => void emit("capture-command", c);

type Bounds = { x?: number; y?: number; w?: number; h?: number };
// Layout is stored per monitor (keyed by `monitorKey`), so each display keeps its own
// arrangement. `size` is a global fallback so a display we have never arranged on still
// inherits a sensible panel size instead of the backend default.
type Store = { v: 2; mon: Record<string, Bounds>; size?: Bounds };
type Mon = Awaited<ReturnType<typeof currentMonitor>>;

// A stable identifier for a monitor across sessions: its name plus geometry. Geometry is
// included because `name` can be null or duplicated, and it pins a remembered layout to a
// physical arrangement - if the resolution or position of a display changes, its old layout
// no longer matches and we fall back to defaults rather than placing panels off-screen.
function monitorKey(m: Mon): string {
  if (!m) return "default";
  return `${m.name || "mon"}@${m.position.x},${m.position.y}:${m.size.width}x${m.size.height}`;
}

// Remember an overlay window's bounds (per label, per display) so the interview layout
// persists across sessions and is independent on each monitor. Restores on open, then saves
// move/resize changes (debounced) under whichever display the panel currently sits on.
// `collapsedRef` (optional): while it reads true, resize events are NOT persisted, so
// collapsing a panel to a tiny icon does not overwrite its real expanded size.
export function usePersistedBounds(collapsedRef?: { current: boolean }) {
  useEffect(() => {
    const win = getCurrentWindow();
    const key = "cap.bounds." + win.label;

    // Load + migrate. v2 keys position/size per monitor; the old format was a flat
    // {x,y,w,h} (one layout shared across all displays) - carry its size as the global
    // fallback and seed it onto whichever display we open on (below).
    let store: Store = { v: 2, mon: {} };
    let legacy: Bounds | null = null;
    try {
      const raw = JSON.parse(localStorage.getItem(key) || "{}");
      if (raw && raw.v === 2 && raw.mon) {
        store = { v: 2, mon: raw.mon, size: raw.size };
      } else if (raw && (typeof raw.w === "number" || typeof raw.x === "number")) {
        legacy = { x: raw.x, y: raw.y, w: raw.w, h: raw.h };
        store.size = { w: raw.w, h: raw.h };
      }
    } catch {
      /* ignore malformed bounds */
    }

    let monitors: Mon[] = [];
    let curKey = "default"; // the display the panel is currently on (updated as it moves)
    let ready = false; // ignore the move/resize events fired by our own restore below
    let t: number | undefined;
    let cancelled = false;
    const uns: UnlistenFn[] = [];

    const persist = () => {
      window.clearTimeout(t);
      t = window.setTimeout(() => localStorage.setItem(key, JSON.stringify(store)), 300);
    };
    // Which saved monitor a physical point falls on (so dragging a panel onto another
    // display records it for THAT display); falls back to the display we opened on.
    const keyAt = (x: number, y: number): string => {
      const m = monitors.find(
        (mm) =>
          !!mm &&
          x >= mm.position.x &&
          x < mm.position.x + mm.size.width &&
          y >= mm.position.y &&
          y < mm.position.y + mm.size.height,
      );
      return m ? monitorKey(m) : curKey;
    };
    const save = (k: string, patch: Bounds) => {
      store.mon[k] = { ...store.mon[k], ...patch };
      if (typeof patch.w === "number" && typeof patch.h === "number") {
        store.size = { w: patch.w, h: patch.h }; // global size fallback for new displays
      }
      persist();
    };

    // Restore this display's saved size always, and its saved position only when it still
    // lands on this monitor. The backend opens the overlay on the display the user chose;
    // restoring per-monitor means a layout saved on another screen never yanks the panel
    // away (the "opens on display 2 then jumps to display 1" bug).
    void (async () => {
      let mon: Mon = null;
      try {
        [mon, monitors] = await Promise.all([currentMonitor(), availableMonitors()]);
      } catch {
        /* no monitor info: degrade to a single shared layout under "default" */
      }
      if (cancelled) return;
      curKey = monitorKey(mon);

      // First run after the format upgrade: seed the old single layout onto this display.
      if (legacy && !store.mon[curKey]) store.mon[curKey] = { ...legacy };

      const b = store.mon[curKey] || {};
      const w = b.w ?? store.size?.w;
      const h = b.h ?? store.size?.h;
      try {
        if (typeof w === "number" && typeof h === "number") {
          await win.setSize(new PhysicalSize(w, h));
        }
        if (typeof b.x === "number" && typeof b.y === "number" && mon) {
          const onMon =
            b.x >= mon.position.x &&
            b.x < mon.position.x + mon.size.width &&
            b.y >= mon.position.y &&
            b.y < mon.position.y + mon.size.height;
          if (onMon) await win.setPosition(new PhysicalPosition(b.x, b.y));
        }
      } catch {
        /* ignore restore errors */
      }
      ready = true;
    })();

    const add = (p: Promise<UnlistenFn>) => void p.then((u) => (cancelled ? u() : uns.push(u)));
    add(
      win.onMoved(({ payload }) => {
        if (!ready) return;
        curKey = keyAt(payload.x, payload.y);
        save(curKey, { x: payload.x, y: payload.y });
      }),
    );
    add(
      win.onResized(({ payload }) => {
        if (!ready || collapsedRef?.current) return; // don't persist the collapsed (icon) size
        save(curKey, { w: payload.width, h: payload.height });
      }),
    );
    return () => {
      cancelled = true;
      window.clearTimeout(t);
      uns.forEach((u) => u());
    };
  }, []);
}
