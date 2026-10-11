import { useRef, useState } from "react";

import { option, Pill } from "./pill";

export interface Language {
  tag: string;
  /** The name of the language, in that language. */
  label: string;
  /** Where the page is in that language. A language with no address is chosen with `onChoose`. */
  href?: string;
}

/** Takes the chosen language. A promise says if the language was kept. */
type Choose = (tag: string) => void | Promise<unknown>;

/**
 * The languages of a realm, at the foot of each page. A realm with one language has no chooser.
 * The chosen language says its name, and the others show their short tag.
 */
export function Languages({
  label,
  languages,
  current,
  onChoose,
}: {
  /** What the chooser is, for a screen reader. */
  label: string;
  languages: Language[];
  current: string;
  onChoose?: Choose | undefined;
}) {
  const [chosen, setChosen] = useState(current);
  // A choice is on its way: a second one waits for its end.
  const waits = useRef(false);
  if (languages.length < 2) return null;
  // Keycloak sorts the languages by their names in the language of the page. Each language keeps
  // its place here, so a person finds it where it was.
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
          // The thumb slides at once, and the page comes back in that language. A language that
          // was not kept gives the thumb back to the language of the page.
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
