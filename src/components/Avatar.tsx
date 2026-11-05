/** Round profile picture, or the name's first letter when none is set. */
export function Avatar({ src, name, size = 28 }: { src: string | null; name: string; size?: number }) {
  if (src) {
    return <img src={src} alt="" width={size} height={size} draggable={false} className="shrink-0 rounded-full object-cover" style={{ width: size, height: size }} />;
  }
  const letter = Array.from(name.trim())[0]?.toUpperCase() ?? "?";
  return (
    <span
      aria-hidden
      className="flex shrink-0 items-center justify-center rounded-full bg-accent font-semibold text-accent-fg select-none"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.45) }}
    >
      {letter}
    </span>
  );
}
