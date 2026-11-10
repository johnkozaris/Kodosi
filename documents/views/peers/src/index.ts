import { colorOf } from "@documents/client";
import type { Peer } from "@documents/contract";
import type { Node as ProseNode } from "@tiptap/pm/model";
import { type EditorState, Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";
import { sendableSteps } from "prosemirror-collab";
import "./peers.css";

export { Waiting } from "./waiting";

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

/** Another person's place in this copy. */
interface Shown extends Place {
  peer: Peer;
}

interface Peers {
  shown: readonly Shown[];
  marks: DecorationSet;
}

const peerKey = new PluginKey<Peers>("peers");

function peerMarks(doc: ProseNode, shown: readonly Shown[]): DecorationSet {
  const decorations: Decoration[] = [];
  for (const { peer, from, to } of shown) {
    const color = colorOf(peer.person);
    if (to > from) {
      decorations.push(Decoration.inline(from, to, { style: `background:${color}33` }));
    }
    const caret = document.createElement("span");
    caret.className = "peer-caret";
    caret.style.borderColor = color;
    const name = caret.appendChild(document.createElement("span"));
    name.textContent = peer.name;
    name.style.background = color;
    decorations.push(Decoration.widget(to, caret, { side: 1, key: `${peer.id} ${peer.name}` }));
  }
  return DecorationSet.create(doc, decorations);
}

/**
 * Where the other people are. Each place moves with every change of this copy, and a peer
 * whose place is new comes in from the document's order.
 */
export function peersPlugin(): Plugin<Peers> {
  return new Plugin<Peers>({
    key: peerKey,
    state: {
      init: () => ({ shown: [], marks: DecorationSet.empty }),
      apply(transaction, current, _old, state) {
        const peers = transaction.getMeta(peerKey) as readonly Peer[] | undefined;
        if (!peers && !transaction.docChanged) return current;
        const moved = transaction.docChanged
          ? current.shown.map((place) => ({
              ...place,
              from: transaction.mapping.map(place.from, 1),
              to: transaction.mapping.map(place.to, 1),
            }))
          : current.shown;
        const shown = peers
          ? peers.flatMap((peer) => {
              const known = moved.find((place) => place.peer === peer);
              if (known) return [known];
              const place = placeOf(peer.place);
              return place ? [{ peer, ...shownPlace(state, place) }] : [];
            })
          : moved;
        return { shown, marks: peerMarks(state.doc, shown) };
      },
    },
    props: {
      decorations(state) {
        return peerKey.getState(state)?.marks;
      },
    },
  });
}

/** Shows the people who are here now, each at their place. */
export function showPeers(view: EditorView, peers: readonly Peer[]): void {
  view.dispatch(view.state.tr.setMeta(peerKey, peers));
}
