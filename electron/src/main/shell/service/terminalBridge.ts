import os from 'os';
import path from 'path';

export function isSafeExternalUrl(input: string): boolean {
  try {
    const url = new URL(input);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

export function expandPath(input: string, cwd: string): string {
  if (input.startsWith('~/')) return path.join(os.homedir(), input.slice(2));
  if (path.isAbsolute(input)) return input;
  return path.resolve(cwd, input);
}
