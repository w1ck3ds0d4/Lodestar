import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import "katex/dist/katex.min.css";

// rehype-katex must not throw: answers stream in token by token, so the markdown is
// briefly malformed (an unclosed `$$`, a half-typed macro). With throwOnError it would
// surface as a render crash; instead render the offending bit in red and keep going.
const KATEX_OPTS = { throwOnError: false, errorColor: "#e06c75" } as const;

// A trailing "Next: ..." heads-up line the live prompt may append: it is metadata for the
// user (the likely follow-up), never something they read aloud, so the live renderer pulls
// it out of the spoken core and shows it dimmed below. Anchored to the end so a half-typed
// line mid-stream is not mistaken for it.
const NEXT_RE = /(?:^|\n)[ \t]*Next:[ \t]*([^\n]+?)[ \t]*$/i;

// Shared answer renderer (GitHub-flavored markdown + KaTeX). Used by the desktop Live
// answers and the capture-mode Answers panel so they render identically. In `live` mode the
// answer is a teleprompter the user reads aloud: the spoken sentences render large, and the
// non-spoken "Next:" hint is split off into a dim chip.
export const Answer = memo(function Answer({ text, live = false }: { text: string; live?: boolean }) {
  let body = text;
  let next: string | null = null;
  if (live) {
    const m = body.match(NEXT_RE);
    if (m) {
      next = m[1];
      body = body.slice(0, m.index).trimEnd();
    }
  }
  return (
    <div className={live ? "assistant-answer assistant-answer--live" : "assistant-answer"}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[
          [rehypeKatex, KATEX_OPTS],
          // ignoreMissing: an unknown language tag (or a half-streamed fence) must not throw.
          [rehypeHighlight, { ignoreMissing: true }],
        ]}
      >
        {body}
      </ReactMarkdown>
      {next && <div className="answer-next">Next: {next}</div>}
    </div>
  );
});
