/** A part of the page in CSS pixels from the top left of the document, as a host pictures it. */
export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The pictures of a rendering: how many there are, then the area of each one. A view may show
 * only the page that it gave last, as a presentation shows one slide.
 */
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

/** Waits until the page drew what it has: its fonts, a pause, then two frames. */
export async function settled(pause = 0): Promise<void> {
  await document.fonts.ready;
  if (pause > 0) await new Promise((done) => setTimeout(done, pause));
  await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
}
