import type { AppMode } from '#main/global/types/lifecycle';

const MODE_COLORS: Partial<Record<AppMode, [number, number, number]>> = {
  dev: [59, 130, 246],
  preview: [168, 85, 247],
  'prod-local': [249, 115, 22],
  sandbox: [156, 163, 175],
};

export function modeColor(mode: AppMode): [number, number, number] {
  const color = MODE_COLORS[mode];
  if (!color) {
    // invariant: every AppMode variant must have a MODE_COLORS entry; missing means a code change
    throw new Error(`No tint color defined for mode: ${mode}`);
  }
  return color;
}

export function fillCircle(
  bitmap: Buffer,
  w: number,
  h: number,
  cx: number,
  cy: number,
  radius: number,
  r: number,
  g: number,
  b: number,
): void {
  const r2 = radius * radius;
  const yMin = Math.max(0, cy - radius);
  const yMax = Math.min(h - 1, cy + radius);
  const xMin = Math.max(0, cx - radius);
  const xMax = Math.min(w - 1, cx + radius);

  for (let y = yMin; y <= yMax; y++) {
    for (let x = xMin; x <= xMax; x++) {
      if ((x - cx) ** 2 + (y - cy) ** 2 <= r2) {
        const idx = (y * w + x) * 4;
        bitmap[idx] = b;
        bitmap[idx + 1] = g;
        bitmap[idx + 2] = r;
        bitmap[idx + 3] = 255;
      }
    }
  }
}
