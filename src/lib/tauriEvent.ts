/**
 * Tauri's event bridge, made safe outside the desktop shell.
 *
 * `listen`/`emit` reach through `window.__TAURI_INTERNALS__`, which only exists inside the
 * Tauri webview. In a plain browser (the Vite dev server / Claude Code preview) the real
 * calls reject with "Cannot read properties of undefined (reading 'transformCallback')" -
 * once per listener, on every mount, which buried genuine errors in the console.
 *
 * `api.ts` already shims `invoke` for the same reason; this is the event half of that.
 * Outside Tauri both become inert: `listen` resolves to a no-op unsubscribe so cleanup
 * still works, and `emit` resolves immediately.
 */
import { emit as tauriEmit, listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export type { UnlistenFn };

const HAS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const noopUnlisten: UnlistenFn = () => {};

export function listen<T>(
  event: string,
  handler: (e: { payload: T }) => void,
): Promise<UnlistenFn> {
  if (!HAS_TAURI) return Promise.resolve(noopUnlisten);
  return tauriListen<T>(event, handler);
}

export function emit(event: string, payload?: unknown): Promise<void> {
  if (!HAS_TAURI) return Promise.resolve();
  return tauriEmit(event, payload);
}
