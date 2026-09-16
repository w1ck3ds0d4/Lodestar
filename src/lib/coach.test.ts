import { describe, expect, it } from "vitest";
import { coachMetrics, wordCount } from "./coach";

const T0 = 1_700_000_000_000;
const you = (text: string, dt = 0) => ({ role: "you" as const, text, ts: T0 + dt });
const them = (text: string, dt = 0) => ({ role: "them" as const, text, ts: T0 + dt });

describe("wordCount", () => {
  it("ignores extra whitespace and empty strings", () => {
    expect(wordCount("  two   words ")).toBe(2);
    expect(wordCount("   ")).toBe(0);
  });
});

describe("coachMetrics", () => {
  it("only measures the user, never the interviewer", () => {
    const m = coachMetrics([them("a very long question from them indeed"), you("short")], T0 + 5000);
    expect(m.myWords).toBe(1);
  });

  it("times only the trailing run, so an interviewer turn resets the clock", () => {
    const segs = [you("first", 0), them("their turn", 10_000), you("second", 20_000)];
    const m = coachMetrics(segs, T0 + 30_000);
    // 10s since the last run started, not 30s since the session did.
    expect(Math.round(m.runSecs)).toBe(10);
    expect(m.speaking).toBe(true);
  });

  it("reports not-speaking when the interviewer holds the floor", () => {
    const m = coachMetrics([you("hi", 0), them("their turn", 1000)], T0 + 2000);
    expect(m.speaking).toBe(false);
    expect(m.runSecs).toBe(0);
    expect(m.wpm).toBeNull();
  });

  it("suppresses wpm until the sample is long enough to be meaningful", () => {
    const m = coachMetrics([you("word")], T0 + 1000);
    expect(m.wpm).toBeNull();
  });

  it("computes pace over the whole run", () => {
    // 30 words across a 30s run = 60wpm.
    const text = Array.from({ length: 30 }, (_, i) => `w${i}`).join(" ");
    const m = coachMetrics([you(text)], T0 + 30_000);
    expect(m.wpm).toBe(60);
  });

  it("counts fillers case-insensitively and across lines", () => {
    const m = coachMetrics([you("Um, basically it's, you know, fine"), you("uh right")], T0 + 5000);
    // um, basically, you know, uh
    expect(m.fillers).toBe(4);
  });

  it("does not flag 'like' or 'actually', which have too many honest uses", () => {
    const m = coachMetrics([you("I actually like systems like this")], T0 + 5000);
    expect(m.fillers).toBe(0);
  });

  it("handles an empty transcript", () => {
    const m = coachMetrics([], T0);
    expect(m).toMatchObject({ myWords: 0, fillers: 0, runSecs: 0, wpm: null, speaking: false });
  });
});
