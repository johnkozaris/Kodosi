import type { Link, TextLook, Theme } from "./text";

export interface Shape {
  id: string;
  name: string;
  kind: string;
  placeholder: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  geometry: string | null;
  fill: string | null;
  line: string | null;
  asset: string | null;
  link: Link | null;
  text: unknown;
  look: TextLook | null;
  order: number;
  source: string | null;
}

export interface Slide {
  id: string;
  position: number;
  shapes: Shape[];
  behind: Shape[];
  background: string | null;
}

export interface Snapshot {
  width: number;
  height: number;
  slides: Slide[];
  theme: Theme | null;
}

export type Fields = Partial<
  Pick<Shape, "x" | "y" | "width" | "height" | "rotation" | "fill" | "line" | "order">
>;

export type Change =
  | { kind: "shape"; slide: string; shape: string; fields: Fields }
  | { kind: "text"; slide: string; shape: string; steps: unknown[] }
  | { kind: "moveSlide"; slide: string; position: number }
  | { kind: "addSlide"; slide: string; position: number }
  | { kind: "removeSlide"; slide: string }
  | { kind: "addShape"; slide: string; shape: Shape }
  | { kind: "removeShape"; slide: string; shape: string }
  | { kind: "group"; changes: Change[] };

/**
 * Reconciles queued edits in order against a reloaded deck snapshot: drops additions that already
 * exist and drops edits to removed slides or shapes, which would otherwise cause the document to
 * refuse the entire batch.
 */
export class Fitting {
  private readonly held: Map<string, Set<string>>;
  /** True if any edit was dropped because its target slide or shape no longer exists. */
  lost = false;

  constructor(slides: readonly Slide[]) {
    this.held = new Map(
      slides.map((slide) => [slide.id, new Set(slide.shapes.map(({ id }) => id))]),
    );
  }

  fit(change: Change): Change | null {
    if (change.kind === "group") {
      const changes = change.changes.flatMap((inner) => this.fit(inner) ?? []);
      return changes.length > 0 ? { kind: "group", changes } : null;
    }
    const shapes = this.held.get(change.slide);
    switch (change.kind) {
      case "addSlide":
        if (shapes) return null;
        this.held.set(change.slide, new Set());
        return change;
      case "removeSlide":
        return this.held.delete(change.slide) ? change : null;
      case "addShape":
        if (!shapes) return this.gone();
        if (shapes.has(change.shape.id)) return null;
        shapes.add(change.shape.id);
        return change;
      case "removeShape":
        return shapes?.delete(change.shape) ? change : null;
      case "moveSlide":
        return shapes ? change : this.gone();
      case "shape":
      case "text":
        return shapes?.has(change.shape) ? change : this.gone();
    }
  }

  private gone(): null {
    this.lost = true;
    return null;
  }
}

export function touches(change: Change, slide: string, shape?: string): boolean {
  if (change.kind === "group") return change.changes.some((inner) => touches(inner, slide, shape));
  if (change.slide !== slide) return false;
  if (shape === undefined) return true;
  if (change.kind === "addShape") return change.shape.id === shape;
  return "shape" in change && change.shape === shape;
}
