export const MIN_ZOOM = 1;
export const MAX_ZOOM = 5;

export function clampZoom(value: number): number {
  return value < MIN_ZOOM ? MIN_ZOOM : value > MAX_ZOOM ? MAX_ZOOM : value;
}

export function formatZoomPercent(zoom: number): string {
  return `${(zoom * 100) | 0}%`;
}
