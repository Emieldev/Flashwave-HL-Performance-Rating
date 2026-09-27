import { Fragment, type ReactNode } from "react";

/**
 * Just enough markdown for the release notes: headings, paragraphs, bullet
 * lists, fenced code, and inline bold, italics and code. Anything else shows
 * as the text it is, which is always readable -- the point of not pulling in
 * a whole markdown library for one panel.
 */
export function Markdown({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "" || /^---+$/.test(line.trim())) {
      i++;
      continue;
    }
    if (line.startsWith("```")) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) code.push(lines[i++]);
      i++;
      blocks.push(<pre key={blocks.length} className="md-code">{code.join("\n")}</pre>);
      continue;
    }
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) {
      blocks.push(<h4 key={blocks.length} className="md-h">{inline(h[2])}</h4>);
      i++;
      continue;
    }
    if (/^\s*[-*]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) {
        let item = lines[i++].replace(/^\s*[-*]\s+/, "");
        // A wrapped item continues on indented lines.
        while (i < lines.length && /^\s{2,}\S/.test(lines[i]) && !/^\s*[-*]\s+/.test(lines[i])) item += " " + lines[i++].trim();
        items.push(item);
      }
      blocks.push(
        <ul key={blocks.length} className="md-list">
          {items.map((it, k) => <li key={k}>{inline(it)}</li>)}
        </ul>,
      );
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !/^(#{1,4}\s|```|\s*[-*]\s+|---+$)/.test(lines[i])) para.push(lines[i++].trim());
    blocks.push(<p key={blocks.length} className="md-p">{inline(para.join(" "))}</p>);
  }
  return <div className="md">{blocks}</div>;
}

/** **bold**, *italic* or _italic_, `code`, and [text](url) shown as its text. */
function inline(s: string): ReactNode {
  const out: ReactNode[] = [];
  const re = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]+\)|(?<![\w*])[*_]([^*_]+)[*_](?![\w*])/g;
  let last = 0;
  for (const m of s.matchAll(re)) {
    if (m.index! > last) out.push(s.slice(last, m.index));
    if (m[1] !== undefined) out.push(<strong key={out.length}>{inline(m[1])}</strong>);
    else if (m[2] !== undefined) out.push(<code key={out.length}>{m[2]}</code>);
    else if (m[3] !== undefined) out.push(m[3]);
    else if (m[4] !== undefined) out.push(<em key={out.length}>{m[4]}</em>);
    last = m.index! + m[0].length;
  }
  if (last < s.length) out.push(s.slice(last));
  return <Fragment>{out}</Fragment>;
}
