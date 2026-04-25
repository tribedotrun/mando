import React from 'react';
import { formatExpiry } from '#renderer/domains/settings/service/formatters';

export function CredentialExpiry({
  expiresAt,
}: {
  expiresAt: number | null;
}): React.ReactElement | null {
  const expiry = formatExpiry(expiresAt);
  if (!expiry) return null;
  return <span className="text-xs text-muted-foreground">{expiry}</span>;
}
