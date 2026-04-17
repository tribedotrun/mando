import type { WebFrameMain } from 'electron';

export function frameUrl(frame: WebFrameMain | null | undefined): string {
  return frame?.url ?? '';
}

export function isTrustedRendererUrl(url: string, origins: ReadonlySet<string>): boolean {
  if (!url) return false;
  try {
    return origins.has(new URL(url).origin);
  } catch {
    return false;
  }
}

export function isTrustedSenderFrame(
  frame: WebFrameMain | null | undefined,
  origins: ReadonlySet<string>,
): boolean {
  if (!frame) return false;
  return isTrustedRendererUrl(frameUrl(frame), origins);
}
