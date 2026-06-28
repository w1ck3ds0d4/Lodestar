import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import "katex/dist/katex.min.css";
import { splitNext } from "../lib/answer";

// rehype-katex must not throw: answers stream in token by token, so the markdown is
// briefly malformed (an unclosed `$$`, a half-typed macro). With throwOnError it would
// surface as a render crash; instead render the offending bit in red and keep going.
const KATEX_OPTS = { throwOnError: false, errorColor: "#e06c75" } as const;

// Shared answer renderer (GitHub-flavored markdown + KaTeX). Used by the desktop Live
// answers and the capture-mode Answers panel so they render identically. In `live` mode the
// answer is a teleprompter the user reads aloud: the spoken sentences render large, and the
// non-spoken trailing "Next:" hint is split off (see lib/answer) into a dim chip.
export const Answer = memo(function Answer({ text, live = false }: { text: string; live?: boolean }) {
  const { body, next } = live ? splitNext(text) : { body: text, next: null };
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
