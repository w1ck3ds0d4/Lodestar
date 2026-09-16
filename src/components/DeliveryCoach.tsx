import { memo, useEffect, useMemo, useState } from "react";
import { Gauge, MessageSquareWarning, Timer } from "lucide-react";
import { coachMetrics, FAST_WPM, LONG_RUN_S, type CoachSegment } from "../lib/coach";

/**
 * Live delivery feedback from the user's OWN transcript lines: how long the current turn
 * has run, how fast they are speaking, and how many fillers they have used.
 *
 * Entirely local - no API calls, no extra spend. The metric maths lives in lib/coach.ts so
 * it is unit-tested. Memoised and self-ticking so the 1s clock re-renders only this strip,
 * never the whole assistant view mid-call.
 */
export const DeliveryCoach = memo(function DeliveryCoach({
  segments,
  active,
}: {
  segments: CoachSegment[];
  active: boolean;
}) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, [active]);

  const m = useMemo(() => coachMetrics(segments, now), [segments, now]);

  if (!active || !m.myWords) return null;

  const longRun = m.speaking && m.runSecs > LONG_RUN_S;
  const tooFast = m.wpm !== null && m.wpm > FAST_WPM;
  const mins = Math.floor(m.runSecs / 60);
  const secs = Math.floor(m.runSecs % 60);

  return (
    <div className="coach" role="status" aria-live="polite">
      {m.speaking && (
        <span
          className={"coach-stat" + (longRun ? " warn" : "")}
          title="How long you have been talking in this turn"
        >
          <Timer size={13} />
          {mins}:{String(secs).padStart(2, "0")}
        </span>
      )}
      {m.wpm !== null && (
        <span className={"coach-stat" + (tooFast ? " warn" : "")} title="Your speaking pace">
          <Gauge size={13} />
          {m.wpm} wpm
        </span>
      )}
      {m.fillers > 0 && (
        <span
          className="coach-stat"
          title="Filler words this session (um, you know, basically...)"
        >
          <MessageSquareWarning size={13} />
          {m.fillers}
        </span>
      )}
      {longRun && <span className="coach-nudge">wrap up</span>}
      {!longRun && tooFast && <span className="coach-nudge">slow down</span>}
    </div>
  );
});
