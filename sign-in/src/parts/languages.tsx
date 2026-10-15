import { useRef, useState } from "react";

import { option, Pill } from "./pill";

export interface Language {
  tag: string;
  label: string;
  href?: string;
}

type Choose = (tag: string) => void | Promise<unknown>;

export function Languages({
  label,
  languages,
  current,
  onChoose,
}: {
  label: string;
  languages: Language[];
  current: string;
  onChoose?: Choose | undefined;
}) {
  const [chosen, setChosen] = useState(current);
  const waits = useRef(false);
  if (languages.length < 2) return null;
  const sorted = languages.toSorted((a, b) => (a.tag < b.tag ? -1 : 1));
  return (
    <nav aria-label={label} className="max-w-full">
      <Pill>
        {sorted.map(({ tag, label: name, href }) => {
          const on = tag === chosen;
          const shared = {
            lang: tag,
            "data-on": on ? "" : undefined,
            "aria-current": on ? ("true" as const) : undefined,
            "aria-label": name,
            className: option(on),
          };
          const choose = () => {
            if (on || waits.current) return;
            setChosen(tag);
            const kept = onChoose?.(tag);
            if (!kept) return;
            waits.current = true;
            void kept
              .catch(() => setChosen(current))
              .finally(() => {
                waits.current = false;
              });
          };
          const short = (tag.split("-")[0] ?? tag).toUpperCase();
          return href ? (
            <a key={tag} href={href} hrefLang={tag} onClick={choose} {...shared}>
              {on ? name : short}
            </a>
          ) : (
            <button key={tag} type="button" onClick={choose} {...shared}>
              {on ? name : short}
            </button>
          );
        })}
      </Pill>
    </nav>
  );
}
