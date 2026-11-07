type Table = Readonly<Record<string, string>>;

const TABLES: Readonly<Record<string, () => Promise<{ default: Table }>>> = {
  de: () => import("./de.json"),
  el: () => import("./el.json"),
  es: () => import("./es.json"),
  fr: () => import("./fr.json"),
  it: () => import("./it.json"),
  ja: () => import("./ja.json"),
  ko: () => import("./ko.json"),
  "zh-Hans": () => import("./zh-Hans.json"),
};

let shown = "en";
let table: Table = {};

export async function useLanguage(language: string | undefined): Promise<void> {
  const load = language ? TABLES[language] : undefined;
  table = load ? (await load()).default : {};
  shown = load && language ? language : "en";
}

export function language(): string {
  return shown;
}

export function say(words: string, values?: Readonly<Record<string, string | number>>): string {
  const text = table[words] ?? words;
  if (!values) return text;
  return text.replace(/\{(\w+)\}/g, (all, name: string) => String(values[name] ?? all));
}
