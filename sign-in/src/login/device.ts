/**
 * The Kodosi app shows a short code while it waits for the browser, and it says: "Check that
 * your browser shows this code." Keycloak has the code on one page only: the page that takes it.
 * That page keeps the code that it sends to Keycloak, in this tab only, so each later step of
 * the sign-in can show it.
 *
 * The pages show a code only when they sent that code to Keycloak themselves. A code from a
 * different place (the address of a later page, for example) is not shown: a link that a stranger
 * made could show one code and connect a different one.
 */
const KEY = "kodosi.code";
/** Keycloak keeps a code for fifteen minutes in the realm of Kodosi. */
const FRESH_MS = 15 * 60 * 1000;
const SHAPE = /^[A-Za-z0-9]{4}-?[A-Za-z0-9]{4}$/;

function shaped(code: string | null | undefined): string | null {
  const letters = (code?.trim() ?? "").toUpperCase();
  if (!SHAPE.test(letters)) return null;
  // Keycloak writes a code as two halves.
  return letters.includes("-") ? letters : `${letters.slice(0, 4)}-${letters.slice(4)}`;
}

/**
 * The code that the app put after the "#" of the address of the code page. A browser keeps that
 * part for itself, so Keycloak gets the code from the page's own form, as from a person who
 * types it.
 */
export function codeFromApp(): string | null {
  try {
    return shaped(decodeURIComponent(location.hash.slice(1)));
  } catch {
    return null;
  }
}

/** Keeps the code that the page sends to Keycloak. */
export function keepCode(code: string | null | undefined) {
  const kept = shaped(code);
  if (!kept) return;
  try {
    sessionStorage.setItem(KEY, JSON.stringify({ code: kept, t: Date.now() }));
  } catch {
    // A browser without storage shows the code on no later page.
  }
}

/** The code of this sign-in, when the visit has one. */
export function heldCode(): string | null {
  try {
    const stored = JSON.parse(sessionStorage.getItem(KEY) ?? "null") as {
      code?: string;
      t?: number;
    } | null;
    if (stored?.code && Date.now() - (stored.t ?? 0) < FRESH_MS) return shaped(stored.code);
  } catch {
    // No code.
  }
  return null;
}

/** The sign-in with this code ended. */
export function dropCode() {
  try {
    sessionStorage.removeItem(KEY);
  } catch {
    // Nothing was kept.
  }
}
