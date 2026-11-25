import { Fragment, type Node as ProseNode, Slice } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { ReplaceAroundStep, ReplaceStep } from "@tiptap/pm/transform";
import { PICTURE_STARTS } from "./generated/writable";
import { cellSpan } from "./schema";

/** Document state used to sanitize pasted content so it stays serializable. */
export interface Holding {
  /** Raw XML/property keys seen in this document and still available to write back. */
  held: ReadonlySet<string>;
  /** Raw keys currently in the document; pasting one of these is a copy, not a cut-move. */
  present: ReadonlySet<string>;
  /** Returns whether the asset bytes are available in an image format the server can write. */
  picture: (asset: string) => boolean;
}

/** Checks magic bytes against image formats the server can embed into a .docx file. */
export function writablePicture(bytes: Uint8Array | undefined): boolean {
  return PICTURE_STARTS.some((start) => start.every((byte, index) => bytes?.[index] === byte));
}

function piecesIn(fragment: Fragment, into = new Set<string>()): Set<string> {
  fragment.descendants((node) => {
    for (const key of [node.attrs.raw, node.attrs.props]) {
      if (typeof key === "string") into.add(key);
    }
  });
  return into;
}

/**
 * Open ancestor nodes in a pasted slice bypass parseHTML, so strip their block IDs, drop foreign
 * raw properties, and clamp table cell spans here.
 */
function fittedAttrs(node: ProseNode, holding: Holding): ProseNode["attrs"] {
  const { id, props, span } = node.attrs;
  const keepsProps = typeof props !== "string" || holding.held.has(props);
  const fits = (id === undefined || id === null) && keepsProps;
  if (fits && (span === undefined || cellSpan(span) === span)) return node.attrs;
  return {
    ...node.attrs,
    ...(id === undefined ? {} : { id: null }),
    ...(keepsProps ? {} : { props: null }),
    ...(span === undefined ? {} : { span: cellSpan(span) }),
  };
}

/**
 * Sanitizes a pasted slice so the document remains saveable. Opaque XML nodes can only be moved
 * via cut-and-paste within the same document; duplicating them would repeat unique OOXML IDs, and
 * foreign ones have no backing XML here, so copies fall back to plain text (or a new image node
 * when the asset bytes are available).
 */
export function fitPaste(slice: Slice, holding: Holding): Slice {
  return new Slice(fitted(slice.content, holding), slice.openStart, slice.openEnd);
}

function fitted(fragment: Fragment, holding: Holding): Fragment {
  const kept: ProseNode[] = [];
  fragment.forEach((node) => {
    const raw = node.attrs.raw;
    const moves = typeof raw === "string" && holding.held.has(raw) && !holding.present.has(raw);
    switch (node.type.name) {
      case "opaque":
        if (moves) kept.push(node);
        return;
      case "inlineOpaque": {
        const shown = String(node.attrs.text).replace(/\s+/g, " ");
        if (moves) kept.push(node);
        else if (shown.trim()) kept.push(node.type.schema.text(shown, node.marks));
        return;
      }
      case "image": {
        const asset = node.attrs.asset;
        if (moves) kept.push(node);
        else if (typeof asset === "string" && holding.picture(asset)) {
          kept.push(node.type.create({ ...node.attrs, raw: null }, null, node.marks));
        }
        return;
      }
    }
    const attrs = fittedAttrs(node, holding);
    if (node.isLeaf) {
      kept.push(attrs === node.attrs ? node : node.type.create(attrs, null, node.marks));
      return;
    }
    const content = fitted(node.content, holding);
    const fits =
      attrs === node.attrs && content.eq(node.content)
        ? node
        : node.type.createAndFill(attrs, content, node.marks);
    if (fits) kept.push(fits);
  });
  return Fragment.from(kept);
}

const HELD = new PluginKey<Set<string>>("held pieces");

export function fitsPastes(picture: (asset: string) => boolean): Plugin<Set<string>> {
  return new Plugin({
    key: HELD,
    state: {
      init: (_, state) => piecesIn(state.doc.content),
      apply(transaction, held) {
        for (const step of transaction.steps) {
          if (step instanceof ReplaceStep || step instanceof ReplaceAroundStep) {
            piecesIn(step.slice.content, held);
          }
        }
        return held;
      },
    },
    props: {
      transformPasted(slice, view) {
        // Internal drag-moves run transformPasted before removing the source nodes from the doc.
        if (view.dragging?.move) return slice;
        return fitPaste(slice, {
          held: HELD.getState(view.state) ?? new Set(),
          present: piecesIn(view.state.doc.content),
          picture,
        });
      },
    },
  });
}
