import type { CredentialInfo } from '#shared/daemon-contract';

export type CredentialSort = 'reset' | 'alphabetical';

function resetTime(credential: CredentialInfo): number {
  const resetAt = credential.sevenDay?.resetAt;
  return resetAt != null && Number.isFinite(resetAt) && resetAt > 0
    ? resetAt
    : Number.POSITIVE_INFINITY;
}

export function sortCredentials(
  credentials: readonly CredentialInfo[],
  sort: CredentialSort,
): CredentialInfo[] {
  return [...credentials].sort((a, b) => {
    if (sort === 'reset') {
      const aReset = resetTime(a);
      const bReset = resetTime(b);
      if (aReset !== bReset) return aReset < bReset ? -1 : 1;
    }
    return (
      a.label.localeCompare(b.label, undefined, { numeric: true, sensitivity: 'base' }) ||
      a.id - b.id
    );
  });
}
