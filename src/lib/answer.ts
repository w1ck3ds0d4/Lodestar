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
