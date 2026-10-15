const KEY = "kodosi.way";

export function lastWay(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function keepWay(alias: string | null) {
  try {
    if (alias) localStorage.setItem(KEY, alias);
    else localStorage.removeItem(KEY);
  } catch {}
}
