import { useEffect, useRef } from "react";

import { type Cursor, liveCursor } from "./cursor";

/**
 * The life of the cursor for one page (cursor.ts). `rest` is a page that is an end: its cursor
 * stays still in the mark. `busy` is a page whose request is on its way to Keycloak.
 */
export function useCursor({ rest, busy }: { rest?: boolean | undefined; busy?: boolean }) {
  const cursor = useRef<Cursor | null>(null);
  useEffect(() => {
    const live = liveCursor();
    cursor.current = live;
    return () => {
      live.destroy();
      cursor.current = null;
    };
  }, []);
  useEffect(() => {
    cursor.current?.waits(!rest);
    cursor.current?.works(!!busy);
  }, [rest, busy]);
  return cursor;
}
