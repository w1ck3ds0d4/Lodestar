import { describe, expect, it } from "vitest";
import { matchPrebuffer, PREBUFFER_TTL_MS, similarity, topicTokens } from "./prebuffer";

const T = 1_700_000_000_000;
const buf = (question: string, answer = "an answer", ts = T) => ({ question, answer, ts });

describe("topicTokens", () => {
  it("drops stopwords and short tokens", () => {
    expect([...topicTokens("How would you scale the database?")]).toEqual(["scale", "database"]);
  });
});

describe("similarity", () => {
  it("is 1 for the same topic words", () => {
    expect(similarity("scale the database", "database scale")).toBe(1);
  });

  it("is 0 for unrelated questions", () => {
    expect(similarity("how would you scale postgres", "tell me about a conflict")).toBe(0);
  });

  it("stays high when the spoken form is wordier than the hint", () => {
    const s = similarity("caching strategy", "so what caching strategy would you reach for here");
    expect(s).toBeGreaterThan(0.5);
  });

  it("does not collide on shared scaffolding alone", () => {
    // Both are "what about X" - only the stopwords overlap, so this must not match.
    expect(similarity("what about kubernetes", "what about testing")).toBe(0);
  });

  it("handles empty input", () => {
    expect(similarity("", "anything")).toBe(0);
  });
});

describe("matchPrebuffer", () => {
  it("serves a fresh, on-topic prediction", () => {
    expect(matchPrebuffer(buf("caching strategy"), "what caching strategy would you use", T)).not.toBeNull();
  });

  it("refuses an off-topic question", () => {
    expect(matchPrebuffer(buf("caching strategy"), "tell me about yourself", T)).toBeNull();
  });

  it("refuses a stale prediction", () => {
    const later = T + PREBUFFER_TTL_MS + 1;
    expect(matchPrebuffer(buf("caching strategy"), "caching strategy", later)).toBeNull();
  });

  it("refuses an empty or missing buffer", () => {
    expect(matchPrebuffer(null, "anything", T)).toBeNull();
    expect(matchPrebuffer(buf("caching", "   "), "caching", T)).toBeNull();
  });
});
