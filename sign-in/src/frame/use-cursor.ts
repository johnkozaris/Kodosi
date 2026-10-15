import { useEffect, useRef } from "react";

import { type Cursor, liveCursor } from "./cursor";

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
