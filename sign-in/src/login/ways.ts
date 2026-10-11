/**
 * The sign-in service that this browser used the last time. A person who signed in with GitHub
 * one month ago does not have to think which of the three it was: that button comes first, with
 * the copper dot. The browser keeps the name of the service and nothing more.
 */
const KEY = "kodosi.way";

export function lastWay(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

/** Keeps the service, or forgets it when the person signs in with a password. */
export function keepWay(alias: string | null) {
  try {
    if (alias) localStorage.setItem(KEY, alias);
    else localStorage.removeItem(KEY);
  } catch {
    // A browser without storage shows the services in the realm's order.
  }
}
