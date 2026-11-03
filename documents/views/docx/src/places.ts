import type { EditorState } from "@tiptap/pm/state";
import { sendableSteps } from "prosemirror-collab";

/** A selection: two positions in the document. */
export interface Place {
  from: number;
  to: number;
}

export function placeOf(body: unknown): Place | null {
  const place = body as Partial<Place> | null;
  if (typeof place?.from !== "number" || typeof place.to !== "number") return null;
  return { from: place.from, to: place.to };
}

/** A place from the document, at the client's rev, in this copy: across its steps that wait. */
export function shownPlace(state: EditorState, place: Place): Place {
  let { from, to } = place;
  for (const step of sendableSteps(state)?.steps ?? []) {
    const map = step.getMap();
    from = map.map(from, 1);
    to = map.map(to, 1);
  }
  const size = state.doc.content.size;
  from = Math.min(from, size);
  return { from, to: Math.min(Math.max(from, to), size) };
}
