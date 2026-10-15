interface Spring {
  stiffness: number;
  damping: number;
  mass?: number;
}

export const spring: Spring = { stiffness: 420, damping: 34, mass: 0.9 };
export const springSoft: Spring = { stiffness: 260, damping: 30 };
export const springSnappy: Spring = { stiffness: 620, damping: 40 };

export function still(): boolean {
  return matchMedia("(prefers-reduced-motion: reduce)").matches;
}

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

export const MOVES = "kodosi:moves";

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
    // A browser without linear() easing throws.
    return node.animate(keyframes, { ...options, easing: "ease-out", ...more });
  }
}
