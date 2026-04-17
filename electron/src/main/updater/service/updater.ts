/** Extract Node.js error code (e.g. 'ENOENT') or empty string. */
export function errCode(err: unknown): string {
  return err instanceof Error && 'code' in err ? ((err as NodeJS.ErrnoException).code ?? '') : '';
}
