/**
 * Mode-aware icon generation for tray and dock.
 *
 * Production: monochrome template (native macOS behavior).
 * Dev:        blue-tinted  (#3b82f6) — matches DevInfoBar.
 * Sandbox:    gray-tinted  (#9ca3af) — matches DevInfoBar.
 *
 * Uses nativeImage bitmap manipulation (BGRA byte order on macOS).
 */
import { nativeImage, NativeImage } from 'electron';
import log from '#main/logger';
import type { AppMode } from '#main/daemon';

// Dev = blue, preview = purple, prod-local = orange (real data, local code), sandbox = gray.
const MODE_COLORS: Record<string, [number, number, number]> = {
  dev: [59, 130, 246],
  preview: [168, 85, 247],
  'prod-local': [249, 115, 22],
  sandbox: [156, 163, 175],
};

/**
 * Tray icon: production uses monochrome template; dev/sandbox are tinted.
 */
export function createTrayIcon(basePath: string, mode: AppMode): NativeImage {
  const base = nativeImage.createFromPath(basePath);
  if (base.isEmpty()) {
    log.error(`Tray icon asset missing: ${basePath}`);
    return base;
  }
  if (mode === 'production') {
    base.setTemplateImage(true);
    return base;
  }

  const [r, g, b] = MODE_COLORS[mode];
  const scaleFactor = basePath.includes('@2x') ? 2 : 1;
  const bitmap = Buffer.from(base.toBitmap());
  const size = base.getSize();
  // getSize() returns logical size; bitmap contains physical pixels.
  const pxW = size.width * scaleFactor;
  const pxH = size.height * scaleFactor;

  // Tint all visible pixels to the mode color.
  for (let i = 0; i < bitmap.length; i += 4) {
    if (bitmap[i + 3] > 0) {
      bitmap[i] = b;
      bitmap[i + 1] = g;
      bitmap[i + 2] = r;
    }
  }

  return nativeImage.createFromBitmap(bitmap, {
    width: pxW,
    height: pxH,
    scaleFactor,
  });
}

/**
 * Dock icon: production is unmodified; dev/sandbox get a colored circle badge.
 */
export function createDockIcon(basePath: string, mode: AppMode): NativeImage {
  const base = nativeImage.createFromPath(basePath);
  if (base.isEmpty()) {
    log.error(`Dock icon asset missing: ${basePath}`);
    return base;
  }
  if (mode === 'production') return base;

  const [r, g, b] = MODE_COLORS[mode];
  const bitmap = Buffer.from(base.toBitmap());
  const size = base.getSize();
  const w = size.width;
  const h = size.height;

  const radius = Math.floor(Math.min(w, h) * 0.13);
  const margin = Math.floor(Math.min(w, h) * 0.05);
  const cx = w - radius - margin;
  const cy = h - radius - margin;

  // White outline ring, then colored fill.
  const outlineRadius = radius + Math.max(3, Math.floor(radius * 0.25));
  fillCircle(bitmap, w, h, cx, cy, outlineRadius, 255, 255, 255);
  fillCircle(bitmap, w, h, cx, cy, radius, r, g, b);

  return nativeImage.createFromBitmap(bitmap, { width: w, height: h });
}

function fillCircle(
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
