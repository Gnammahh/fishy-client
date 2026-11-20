export type WaveOptions = {
  /** Path width in px. Must be a whole number of wavelengths so tiles join seamlessly. */
  width: number;
  /** Height of the drawing area; the wave is filled down to this line. */
  height: number;
  /** Resting y of the wave edge. */
  baseY: number;
  amplitude: number;
  wavelength: number;
  /** Pixel step: every vertex lands on a multiple of this. */
  step: number;
};

const snap = (v: number, step: number) => Math.round(v / step) * step;

/**
 * SVG path `d` for a filled wave whose edge is a sine wave quantized into
 * `step`-px stairs. Starts bottom-left, follows the edge, closes bottom-right.
 */
export function steppedWavePath({ width, height, baseY, amplitude, wavelength, step }: WaveOptions): string {
  if (width % wavelength !== 0) {
    throw new Error(`width ${width} must be a whole number of wavelengths (${wavelength})`);
  }
  const yAt = (x: number) => snap(baseY + amplitude * Math.sin((2 * Math.PI * x) / wavelength), step);

  let y = yAt(0);
  const parts = [`M0,${height}`, `L0,${y}`];
  for (let x = step; x <= width; x += step) {
    const ny = yAt(x);
    if (ny !== y) {
      parts.push(`L${x},${y}`, `L${x},${ny}`);
      y = ny;
    }
  }
  parts.push(`L${width},${y}`, `L${width},${height} Z`);
  return parts.join(" ");
}
