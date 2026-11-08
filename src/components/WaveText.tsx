import type { CSSProperties } from "react";

/** Text whose characters hop and light up one after another, left to right. */
export function WaveText({ text, className = "" }: { text: string; className?: string }) {
  return (
    <span className={className} aria-label={text}>
      {Array.from(text).map((ch, i) => (
        <span key={i} aria-hidden className="wave-char" style={{ "--i": i } as CSSProperties}>
          {ch}
        </span>
      ))}
    </span>
  );
}
