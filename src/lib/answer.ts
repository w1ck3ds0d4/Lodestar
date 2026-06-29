// Pure helpers for the live-answer "NONE" sentinel and the trailing "Next:" hint, shared by
// the streaming answer loop (AssistantView) and the renderer (MarkdownAnswer) and unit-tested
// in answer.test.ts.

// A trailing "Next: <hint>" line the live prompt may append - metadata for the user (the
// likely follow-up), never something read aloud. Anchored to the end so a half-typed line
// mid-stream is not mistaken for it.
const NEXT_RE = /(?:^|\n)[ \t]*Next:[ \t]*([^\n]+?)[ \t]*$/i;

// Letters-only, uppercased, so the "NONE" sentinel survives punctuation/whitespace and stays
// distinguishable from a real answer even mid-stream.
export const strip = (s: string): string => s.replace(/[^a-z]/gi, "").toUpperCase();

// True while the accumulating text could still be heading toward the bare "NONE" sentinel
// (N / NO / NON / NONE), so the answer bubble is held until the stream diverges from it.
export const looksNone = (acc: string): boolean => "NONE".startsWith(strip(acc));

// Split a trailing "Next:" hint off the spoken body; { body, next } (next is null if absent).
export function splitNext(text: string): { body: string; next: string | null } {
  const m = text.match(NEXT_RE);
  if (!m || m.index === undefined) return { body: text, next: null };
  return { body: text.slice(0, m.index).trimEnd(), next: m[1] };
}

// True if a finished answer is really the NONE filler sentinel, ignoring any trailing "Next:"
// line - so a stray hint can never make a filler reply look like a real answer.
export function isNoneAnswer(full: string): boolean {
  const core = splitNext(full).body.trim();
  return core.length === 0 || strip(core) === "NONE";
}

// Acknowledgement / filler words the interviewer drops between real questions. Garbled ASR of
// a halting, non-native speaker often emits these as standalone "Them:" turns; answering them
// produces noise and duplicate cards, which is the main way a bad-English interviewer breaks
// the live assistant. Kept deliberately small (only words that are NEVER a question on their
// own) so a real short question is never swallowed.
const BACKCHANNEL = new Set([
  "ok",
  "okay",
  "kay",
  "k",
  "yeah",
  "yes",
  "yep",
  "yup",
  "ya",
  "no",
  "nope",
  "nah",
  "right",
  "sure",
  "cool",
  "nice",
  "great",
  "perfect",
  "good",
  "okok",
  "mhmm",
  "mhm",
  "mm",
  "mmm",
  "hmm",
  "uh",
  "um",
  "uhuh",
  "aha",
  "ah",
  "oh",
  "alright",
  "exactly",
  "correct",
  "true",
  "fine",
  "gotcha",
  "thanks",
  "thank",
  "you",
  "welcome",
  "bye",
  "hello",
  "hi",
  "hey",
  "so",
  "and",
  "well",
]);

// True if the interviewer text is only acknowledgement/filler words (or too short to carry a
// question), so it should never trigger a live answer. Bounded at 6 words: anything longer is
// plausibly a real (if broken) question and is left for the model to judge.
export function isBackchannel(text: string): boolean {
  const cleaned = text
    .toLowerCase()
    .replace(/[^a-z\s]/g, " ")
    .trim();
  if (cleaned.length === 0) return true;
  const words = cleaned.split(/\s+/);
  if (words.length > 6) return false;
  return words.every((w) => BACKCHANNEL.has(w));
}
