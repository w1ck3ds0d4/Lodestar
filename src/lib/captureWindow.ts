import { useEffect } from "react";
import { emit, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
      if (typeof acc.w === "number" && typeof acc.h === "number") {
        void win.setSize(new PhysicalSize(acc.w, acc.h));
      }
      if (typeof acc.x === "number" && typeof acc.y === "number") {
        void win.setPosition(new PhysicalPosition(acc.x, acc.y));
      }
    } catch {
      /* ignore malformed bounds */
    }

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
