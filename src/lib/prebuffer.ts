/**
 * Matching for the predicted-follow-up pre-buffer.
 *
 * The live prompt ends an answer with a "Next: <likely follow-up>" hint. We speculatively
 * answer that hint so the reply is already on screen if it is actually asked. The risk is
 * serving a confidently WRONG answer, so the match is deliberately conservative and the
 * result is always labelled as predicted until the real answer replaces it.
 */

// Words carrying no topic signal. Dropping them stops "what about X" and "what about Y"
// from looking near-identical just because they share the scaffolding.
const STOP = new Set([
  "a", "an", "the", "is", "are", "was", "were", "be", "been", "do", "does", "did", "you",
  "your", "yours", "me", "my", "i", "we", "our", "us", "to", "of", "in", "on", "at", "for",
  "and", "or", "but", "how", "what", "why", "when", "where", "which", "who", "can", "could",
  "would", "should", "about", "with", "it", "its", "that", "this", "these", "those", "have",
  "has", "had", "will", "just", "any", "some", "there", "then", "than", "so", "if",
]);

/** Topic tokens: lowercased alphanumerics, 3+ chars, minus stopwords. */
export function topicTokens(s: string): Set<string> {
  return new Set(
    (s.toLowerCase().match(/[a-z0-9]+/g) ?? []).filter((w) => w.length > 2 && !STOP.has(w)),
  );
}

/**
 * Dice coefficient over topic tokens: 0 (nothing shared) .. 1 (identical topics).
 * Dice rather than Jaccard because it is more forgiving of one side being longer, which is
 * the common case here (a terse predicted hint vs. a rambling spoken question).
 */
export function similarity(a: string, b: string): number {
  const A = topicTokens(a);
  const B = topicTokens(b);
  if (!A.size || !B.size) return 0;
  let shared = 0;
  for (const t of A) if (B.has(t)) shared++;
  return (2 * shared) / (A.size + B.size);
}

/**
 * Serve threshold. Tuned high enough that unrelated questions never collide, but low
 * enough that a 6-word hint still matches the spoken form of the same question. A miss
 * costs a couple of seconds; a false hit costs a wrong answer mid-interview, so when in
 * doubt this should stay strict.
 */
export const PREDICT_MATCH = 0.5;

/** A prediction older than this is stale - the conversation has moved on. */
export const PREBUFFER_TTL_MS = 5 * 60_000;

export interface Prebuffered {
  question: string;
  answer: string;
  ts: number;
}

/** The buffered answer if it is fresh AND confidently about the same thing, else null. */
export function matchPrebuffer(
  buf: Prebuffered | null,
  question: string,
  now: number,
): Prebuffered | null {
  if (!buf || !buf.answer.trim()) return null;
  if (now - buf.ts > PREBUFFER_TTL_MS) return null;
  return similarity(buf.question, question) >= PREDICT_MATCH ? buf : null;
}
