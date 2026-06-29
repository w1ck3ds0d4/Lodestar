import { describe, expect, it } from "vitest";
import { isBackchannel, isNoneAnswer, looksNone, splitNext, strip } from "./answer";

describe("strip", () => {
  it("keeps only letters and uppercases them", () => {
    expect(strip(" n.o.n.e ")).toBe("NONE");
    expect(strip("True.")).toBe("TRUE");
    expect(strip("123 !!")).toBe("");
  });
});

describe("looksNone (mid-stream hold)", () => {
  it("holds while the prefix could still become NONE", () => {
    expect(looksNone("")).toBe(true); // nothing yet
    expect(looksNone("N")).toBe(true);
    expect(looksNone("no")).toBe(true);
    expect(looksNone("NONE")).toBe(true);
  });
  it("releases once the stream diverges from NONE", () => {
    expect(looksNone("No, I'd")).toBe(false); // real answer starting with "No"
    expect(looksNone("I'd start with")).toBe(false);
  });
});

describe("splitNext", () => {
  it("pulls a trailing Next: hint off the body", () => {
    const { body, next } = splitNext("I'd cache the hot path.\n\nNext: monitoring metrics");
    expect(body).toBe("I'd cache the hot path.");
    expect(next).toBe("monitoring metrics");
  });
  it("leaves a body with no Next: line untouched", () => {
    const { body, next } = splitNext("Just a normal answer.");
    expect(body).toBe("Just a normal answer.");
    expect(next).toBeNull();
  });
  it("only treats a Next: line at the very end as the hint", () => {
    // "Next:" earlier in the prose must not be stripped.
    const { next } = splitNext("Next: do X first.\nThen I'd verify it.");
    expect(next).toBeNull();
  });
});

describe("isNoneAnswer (filler detection)", () => {
  it("detects the bare NONE sentinel through punctuation/whitespace", () => {
    expect(isNoneAnswer("NONE")).toBe(true);
    expect(isNoneAnswer(" none. ")).toBe(true);
    expect(isNoneAnswer("")).toBe(true);
  });
  it("ignores a trailing Next: line when deciding NONE", () => {
    expect(isNoneAnswer("NONE\nNext: something")).toBe(true);
  });
  it("does not flag a real answer as NONE", () => {
    expect(isNoneAnswer("I'd use cache-aside here.")).toBe(false);
    expect(isNoneAnswer("None of the replicas were healthy, so I'd...")).toBe(false);
  });
});

describe("isBackchannel (live filler turns)", () => {
  it("treats pure acknowledgement/filler turns as backchannel", () => {
    for (const t of ["Okay.", "Yeah.", "Mhmm.", "Right.", "Okay perfect", "yes yes", "No.", "Alright."]) {
      expect(isBackchannel(t)).toBe(true);
    }
  });
  it("treats empty / punctuation-only turns as backchannel", () => {
    expect(isBackchannel("")).toBe(true);
    expect(isBackchannel("...")).toBe(true);
  });
  it("does not swallow a real (even short) question", () => {
    expect(isBackchannel("Why did you leave?")).toBe(false);
    expect(isBackchannel("What is your notice period?")).toBe(false);
    expect(isBackchannel("Tell me about a conflict.")).toBe(false);
  });
  it("lets a long broken-English turn through for the model to judge", () => {
    expect(isBackchannel("no my communications because at this moment was not good")).toBe(false);
  });
});
