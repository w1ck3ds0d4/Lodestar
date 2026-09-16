/** Delivery metrics computed from the user's own transcript lines. Pure + local. */

export interface CoachSegment {
  role: "you" | "them";
  text: string;
  ts: number;
}

export interface CoachMetrics {
  /** Total words the user has spoken this session. */
  myWords: number;
  /** Filler words the user has used this session. */
  fillers: number;
  /** Seconds the user has been talking in the current uninterrupted turn. */
  runSecs: number;
  /** Pace for the current turn, or null while the sample is too short to be meaningful. */
  wpm: number | null;
  /** Whether the user is the one currently holding the floor. */
  speaking: boolean;
}

// Conservative English filler set: tokens that are almost always filler in speech, so the
// count stays believable. "like"/"actually" are deliberately excluded - both have far too
// many legitimate uses to flag without nagging.
const FILLERS = /\b(?:um+|uh+|er+|ah+|hmm+|you know|i mean|sort of|kind of|basically|literally)\b/gi;

/** A spoken answer is capped at ~70 words / ~25s by the prompt; past this the turn ran away. */
export const LONG_RUN_S = 75;
/** Where a nervous reader starts to blur words together. */
export const FAST_WPM = 175;
/** Below this the rate is noise: one short word can read as 300wpm. */
const MIN_RATE_SAMPLE_S = 4;

export const wordCount = (s: string): number => (s.trim().match(/\S+/g) ?? []).length;

export function coachMetrics(segments: CoachSegment[], now: number): CoachMetrics {
  const mine = segments.filter((s) => s.role === "you");
  const myWords = mine.reduce((n, s) => n + wordCount(s.text), 0);
  const fillers = mine.reduce((n, s) => n + (s.text.match(FILLERS) ?? []).length, 0);

  // The trailing run of consecutive user lines is the turn being spoken right now.
  const run: CoachSegment[] = [];
  for (let i = segments.length - 1; i >= 0 && segments[i].role === "you"; i--) {
    run.unshift(segments[i]);
  }
  const runWords = run.reduce((n, s) => n + wordCount(s.text), 0);
  const runSecs = run.length ? Math.max(0, (now - run[0].ts) / 1000) : 0;
  const wpm = run.length && runSecs >= MIN_RATE_SAMPLE_S ? Math.round(runWords / (runSecs / 60)) : null;

  return { myWords, fillers, runSecs, wpm, speaking: run.length > 0 };
}
