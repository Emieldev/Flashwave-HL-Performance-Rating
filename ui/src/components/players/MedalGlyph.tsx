/**
 * ETF2L's own Highlander medal (Flashy): the in-game item's backpack icon
 * from the TF2 wiki (Tournament Medal - ETF2L Highlander, Season 17 on),
 * cut to 128 px in `public/medals`. The game has one picture for every
 * place; silver and bronze are the same medal tinted, on their own tile.
 */
export function MedalGlyph({ size = 22, place }: { size?: number; place?: number }) {
  const kind = place === 2 ? "silver" : place === 3 ? "bronze" : "gold";
  return <img src="/medals/etf2l-highlander.png" width={size} height={size} alt="" className={`medal-img medal-${kind}`} draggable={false} />;
}
