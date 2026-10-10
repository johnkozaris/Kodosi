/** A spring of the apps (AppTheme.Motion, KodosiTheme): the pages move as the apps move. */
export interface Spring {
  stiffness: number;
  damping: number;
  mass?: number;
}

/** The default spring: it settles quickly with a hint of follow-through. */
export const spring: Spring = { stiffness: 420, damping: 34, mass: 0.9 };
/** Large surfaces: the capsule, the mark. */
export const springSoft: Spring = { stiffness: 260, damping: 30 };
/** Small controls: thumbs, signs. */
export const springSnappy: Spring = { stiffness: 620, damping: 40 };

/** The person asked for less motion. */
export function still(): boolean {
  return matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** A spring as a timing that the browser plays without a script for each frame. */
function springTiming({ stiffness, damping, mass = 1 }: Spring): KeyframeAnimationOptions {
  const step = 1 / 120;
  const points = [0];
  let place = 0;
  let speed = 0;
  for (let time = 0; time < 3; time += step) {
    speed += ((-stiffness * (place - 1) - damping * speed) / mass) * step;
    place += speed * step;
    points.push(place);
    if (Math.abs(1 - place) < 0.0005 && Math.abs(speed) < 0.005) break;
  }
  points[points.length - 1] = 1;
  const kept = points.filter((_, i) => i % 2 === 0 || i === points.length - 1);
  return {
    duration: (points.length - 1) * step * 1000,
    easing: `linear(${kept.map((point) => point.toFixed(4)).join(",")})`,
  };
}

/** The name of the event that says: a thing on the page starts to move. The cursor hears it, and
    it follows what it stands on (cursor.ts). */
export const MOVES = "kodosi:moves";

/**
 * Plays keyframes on a spring. A browser that knows no spring timing plays them eased, and a
 * person who asked for less motion gets the last keyframe at once.
 */
export function play(
  node: Element,
  keyframes: Keyframe[] | PropertyIndexedKeyframes,
  timing: Spring,
  more?: KeyframeAnimationOptions,
): Animation {
  const options = still() ? { duration: 0 } : springTiming(timing);
  document.dispatchEvent(new Event(MOVES));
  try {
    return node.animate(keyframes, { ...options, ...more });
  } catch {
    return node.animate(keyframes, { ...options, easing: "ease-out", ...more });
  }
}
