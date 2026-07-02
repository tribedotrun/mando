import type { AppMode } from '#main/global/types/lifecycle';

interface ParsedSemver {
  major: number;
  minor: number;
  patch: number;
  prerelease: string[];
}

const SEMVER_PATTERN = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;

function parseSemver(version: string): ParsedSemver | null {
  const match = version.trim().match(SEMVER_PATTERN);
  if (!match) return null;

  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] ? match[4].split('.') : [],
  };
}

function comparePrereleaseIdentifier(a: string, b: string): number {
  const aNumber = /^\d+$/.test(a);
  const bNumber = /^\d+$/.test(b);

  if (aNumber && bNumber) return Number(a) - Number(b);
  if (aNumber) return -1;
  if (bNumber) return 1;
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

function comparePrerelease(a: string[], b: string[]): number {
  if (a.length === 0 && b.length === 0) return 0;
  if (a.length === 0) return 1;
  if (b.length === 0) return -1;

  const max = Math.max(a.length, b.length);
  for (let i = 0; i < max; i++) {
    const aPart = a[i];
    const bPart = b[i];
    if (aPart === undefined) return -1;
    if (bPart === undefined) return 1;

    const diff = comparePrereleaseIdentifier(aPart, bPart);
    if (diff !== 0) return diff;
  }
  return 0;
}

export function compareSemver(a: string, b: string): number {
  const pa = parseSemver(a);
  const pb = parseSemver(b);

  if (!pa && !pb) return a === b ? 0 : a < b ? -1 : 1;
  if (!pa) return -1;
  if (!pb) return 1;

  for (const field of ['major', 'minor', 'patch'] as const) {
    const diff = pa[field] - pb[field];
    if (diff !== 0) return diff;
  }
  return comparePrerelease(pa.prerelease, pb.prerelease);
}

export function getAppTitle(mode: AppMode): string {
  if (mode === 'dev') return 'Mando (Dev)';
  if (mode === 'preview') return 'Mando (Preview)';
  if (mode === 'prod-local') return 'Mando (Prod Local)';
  if (mode === 'sandbox') return 'Mando (Sandbox)';
  return 'Mando';
}

export function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err: unknown) {
    // EPERM = process exists but we lack permission to signal it — still alive.
    if ((err as NodeJS.ErrnoException).code === 'EPERM') return true;
    return false;
  }
}
