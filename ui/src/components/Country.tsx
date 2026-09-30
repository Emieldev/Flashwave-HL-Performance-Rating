import { country } from "../lib/countries";

/** A country's flag and name, as ETF2L gives it ("UnitedKingdom"). */
export function Country({ raw, nameless = false }: { raw: string; nameless?: boolean }) {
  const c = country(raw);
  return (
    <span className="country" title={nameless ? c.name : undefined}>
      {c.code && <img className="flag" src={`/flags/${c.code}.png`} alt="" width={20} height={15} />}
      {!nameless && <span>{c.name}</span>}
    </span>
  );
}
