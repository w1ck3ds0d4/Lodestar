import { afterEach, describe, expect, it, vi } from "vitest";

// Record what reaches the REAL Tauri bridge. Hoisted so the mock factory can see it.
const calls = vi.hoisted(() => ({ listen: [] as unknown[][], emit: [] as unknown[][] }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (...args: unknown[]) => {
    calls.listen.push(args);
    return () => {};
  },
  emit: async (...args: unknown[]) => {
    calls.emit.push(args);
  },
}));

const g = globalThis as { window?: unknown };

/** Import a fresh copy, since HAS_TAURI is decided once at module load. */
async function load(insideTauri: boolean) {
  vi.resetModules();
  if (insideTauri) g.window = { __TAURI_INTERNALS__: {} };
  else delete g.window;
  return import("./tauriEvent");
}

afterEach(() => {
  delete g.window;
  calls.listen.length = 0;
  calls.emit.length = 0;
});

describe("inside the desktop app", () => {
  it("delegates listen to the real bridge with the same event and handler", async () => {
    // This is the path the screen reader's result travels: Alt+Shift+R emits "window-text".
    const { listen } = await load(true);
    const handler = () => {};
    await listen("window-text", handler);
    expect(calls.listen).toEqual([["window-text", handler]]);
  });

  it("delegates emit to the real bridge", async () => {
    const { emit } = await load(true);
    await emit("capture-command", { cmd: "listen" });
    expect(calls.emit).toEqual([["capture-command", { cmd: "listen" }]]);
  });
});

describe("in a plain browser (preview)", () => {
  it("never touches the real bridge", async () => {
    const { listen, emit } = await load(false);
    await listen("window-text", () => {});
    await emit("anything");
    expect(calls.listen).toHaveLength(0);
    expect(calls.emit).toHaveLength(0);
  });

  it("still returns a callable unsubscribe so cleanup code works", async () => {
    const { listen } = await load(false);
    const unlisten = await listen("window-text", () => {});
    expect(() => unlisten()).not.toThrow();
  });
});
