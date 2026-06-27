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

// Shared answer renderer (GitHub-flavored markdown + KaTeX). Used by both the desktop
// Live answers and the capture-mode Answers panel so they render identically.
export const Answer = memo(function Answer({ text }: { text: string }) {
  return (
    <div className="assistant-answer">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[
          [rehypeKatex, KATEX_OPTS],
          // ignoreMissing: an unknown language tag (or a half-streamed fence) must not throw.
          [rehypeHighlight, { ignoreMissing: true }],
        ]}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
