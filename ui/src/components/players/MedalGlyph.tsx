/**
 * A medal, drawn (Flashy): a disc on a ribbon in the medal's own colour,
 * for the gold, silver and bronze tiles. Drawn rather than fetched, so it
 * needs no image; `currentColor` is the tile's medal colour.
 */
export function MedalGlyph({ size = 22, place }: { size?: number; place?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden className="medal-glyph">
      {/* The ribbon: two straps meeting behind the disc. */}
      <path d="M7 2h4l2.2 6.4-3.4 1.6z" fill="currentColor" opacity="0.55" />
      <path d="M17 2h-4l-2.2 6.4 3.4 1.6z" fill="currentColor" opacity="0.8" />
      {/* The disc, with a rim and the place on it. */}
      <circle cx="12" cy="15" r="7" fill="currentColor" />
      <circle cx="12" cy="15" r="5.3" fill="none" stroke="#1b1714" strokeOpacity="0.28" strokeWidth="1" />
      {place !== undefined && (
        <text x="12" y="18.4" textAnchor="middle" fontSize="8.5" fontWeight="800" fill="#1b1714" fillOpacity="0.75">
          {place}
        </text>
      )}
    </svg>
  );
}
