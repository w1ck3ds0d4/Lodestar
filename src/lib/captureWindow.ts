import { useEffect } from "react";
import { emit, type UnlistenFn } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition, PhysicalSize } from "@tauri-apps/api/dpi";
import type { CaptureCommand } from "../types";

// Send a command from a capture overlay window back to the main window.
export const sendCaptureCommand = (c: CaptureCommand) => void emit("capture-command", c);

// Remember an overlay window's bounds (per label) so the interview layout persists
// across sessions. Restores on open, then saves move/resize changes (debounced).
export function usePersistedBounds() {
  useEffect(() => {
    const win = getCurrentWindow();
    const key = "cap.bounds." + win.label;
    const acc: { x?: number; y?: number; w?: number; h?: number } = {};
    try {
      Object.assign(acc, JSON.parse(localStorage.getItem(key) || "{}"));
    } catch {
      /* ignore malformed bounds */
    }

    // Restore the saved size always, but the saved POSITION only when it lands on the
    // monitor this panel was just opened on. The backend places the overlay on the
    // display the user chose; without this guard a position saved in a previous session
    // on another display would immediately yank the panel back to that screen (the
    // "opens on display 2 then jumps to display 1" bug).
    void (async () => {
      try {
        if (typeof acc.w === "number" && typeof acc.h === "number") {
          await win.setSize(new PhysicalSize(acc.w, acc.h));
        }
        if (typeof acc.x === "number" && typeof acc.y === "number") {
          const mon = await currentMonitor();
          const onChosenMonitor =
            !!mon &&
            acc.x >= mon.position.x &&
            acc.x < mon.position.x + mon.size.width &&
            acc.y >= mon.position.y &&
            acc.y < mon.position.y + mon.size.height;
          if (onChosenMonitor) {
            await win.setPosition(new PhysicalPosition(acc.x, acc.y));
          }
        }
      } catch {
        /* ignore restore errors */
      }
    })();

    let t: number | undefined;
    const save = (patch: Partial<typeof acc>) => {
      Object.assign(acc, patch);
      window.clearTimeout(t);
      t = window.setTimeout(() => localStorage.setItem(key, JSON.stringify(acc)), 300);
    };
    let cancelled = false;
    const uns: UnlistenFn[] = [];
    const add = (p: Promise<UnlistenFn>) => void p.then((u) => (cancelled ? u() : uns.push(u)));
    add(win.onMoved(({ payload }) => save({ x: payload.x, y: payload.y })));
    add(win.onResized(({ payload }) => save({ w: payload.width, h: payload.height })));
    return () => {
      cancelled = true;
      window.clearTimeout(t);
      uns.forEach((u) => u());
    };
  }, []);
}
