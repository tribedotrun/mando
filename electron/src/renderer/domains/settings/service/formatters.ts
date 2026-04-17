export function formatExpiry(expiresAt: number | null): string | null {
  if (expiresAt == null) return null;
  const now = Date.now();
  const diff = expiresAt - now;
  if (diff <= 0) return 'Expired';
  const hours = (diff / (1000 * 60 * 60)) | 0;
  if (hours < 24) return `${hours}h remaining`;
  const days = (hours / 24) | 0;
  return `${days}d remaining`;
}
