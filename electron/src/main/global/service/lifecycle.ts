import type { AppMode } from '#main/global/types/lifecycle';

export function compareSemver(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] || 0) - (pb[i] || 0);
    if (diff !== 0) return diff;
  }
  return 0;
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
