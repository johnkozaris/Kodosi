/** Bounding box in CSS pixels relative to the document top-left. */
export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Calling picture(index) may switch the active page (e.g. a slide or sheet) to capture it. */
export interface Pictures {
  pictures(): Promise<number>;
  picture(index: number): Promise<Area>;
}

export function areaOf(element: Element): Area {
  const rect = element.getBoundingClientRect();
  return {
    x: rect.left + window.scrollX,
    y: rect.top + window.scrollY,
    width: rect.width,
    height: rect.height,
  };
}

export async function settled(pause = 0): Promise<void> {
  await document.fonts.ready;
  if (pause > 0) await new Promise((done) => setTimeout(done, pause));
  await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
}
