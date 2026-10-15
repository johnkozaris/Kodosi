// A page shows only a code that this tab sent to Keycloak itself: a link from a stranger could
// show one code and connect a different one.
const KEY = "kodosi.code";
const FRESH_MS = 15 * 60 * 1000;
const SHAPE = /^[A-Za-z0-9]{4}-?[A-Za-z0-9]{4}$/;

function shaped(code: string | null | undefined): string | null {
  const letters = (code?.trim() ?? "").toUpperCase();
  if (!SHAPE.test(letters)) return null;
  return letters.includes("-") ? letters : `${letters.slice(0, 4)}-${letters.slice(4)}`;
}

export function codeFromApp(): string | null {
  try {
    return shaped(decodeURIComponent(location.hash.slice(1)));
  } catch {
    return null;
  }
}

export function keepCode(code: string | null | undefined) {
  const kept = shaped(code);
  if (!kept) return;
  try {
    sessionStorage.setItem(KEY, JSON.stringify({ code: kept, t: Date.now() }));
  } catch {}
}

export function heldCode(): string | null {
  try {
    const stored = JSON.parse(sessionStorage.getItem(KEY) ?? "null") as {
      code?: string;
      t?: number;
    } | null;
    if (stored?.code && Date.now() - (stored.t ?? 0) < FRESH_MS) return shaped(stored.code);
  } catch {}
  return null;
}

export function dropCode() {
  try {
    sessionStorage.removeItem(KEY);
  } catch {}
}
