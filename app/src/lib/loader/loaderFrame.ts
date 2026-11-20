export type LoaderPhase = "fall" | "swim" | "eat" | "return";

export type LoaderFrame = {
  phase: LoaderPhase;
  /** Where the fish's mouth is heading, as a 0–1 fraction of the tank: 0.5 = centre, pelletX = at the pellet. */
  fishX: number;
  facing: "left" | "right";
  mouthOpen: boolean;
  /** 0 = top of the tank, 1 = at the fish's mouth. */
  pelletY: number;
  pelletVisible: boolean;
};

export const CYCLE_MS = 1800;

// Phase boundaries in ms, matching the approved mockup (0.3 / 0.55 / 0.625 / 0.7 of the cycle).
const FALL_END = 540;
const SWIM_END = 990;
const BITE_AT = 1125;
const EAT_END = 1260;

const CENTRE = 0.5;
const ease = (q: number) => (q < 0.5 ? 2 * q * q : 1 - 2 * (1 - q) * (1 - q));
const lerp = (a: number, b: number, q: number) => a + (b - a) * q;

export function fishLoaderFrame(elapsedMs: number, pelletX: number): LoaderFrame {
  const t = ((elapsedMs % CYCLE_MS) + CYCLE_MS) % CYCLE_MS;
  const towardPellet = pelletX >= CENTRE ? "right" : "left";

  if (t < FALL_END) {
    return { phase: "fall", fishX: CENTRE, facing: towardPellet, mouthOpen: false, pelletY: t / FALL_END, pelletVisible: true };
  }
  if (t < SWIM_END) {
    const q = ease((t - FALL_END) / (SWIM_END - FALL_END));
    return { phase: "swim", fishX: lerp(CENTRE, pelletX, q), facing: towardPellet, mouthOpen: false, pelletY: 1, pelletVisible: true };
  }
  if (t < EAT_END) {
    const bitten = t >= BITE_AT;
    return { phase: "eat", fishX: pelletX, facing: towardPellet, mouthOpen: !bitten, pelletY: 1, pelletVisible: !bitten };
  }
  const q = ease((t - EAT_END) / (CYCLE_MS - EAT_END));
  const towardCentre = pelletX <= CENTRE ? "right" : "left";
  return { phase: "return", fishX: lerp(pelletX, CENTRE, q), facing: towardCentre, mouthOpen: false, pelletY: 1, pelletVisible: false };
}
