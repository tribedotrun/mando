import React from 'react';
import type { CredentialInfo } from '#renderer/domains/settings/runtime/hooks';

export function StatusBadge({ cred }: { cred: CredentialInfo }): React.ReactElement {
  if (cred.isExpired) {
    return (
      <span className="inline-flex items-center rounded-full bg-destructive/10 px-2 py-0.5 text-xs text-destructive">
        Expired
      </span>
    );
  }
  if (cred.isRateLimited) {
    return (
      <span className="inline-flex items-center rounded-full bg-warning/10 px-2 py-0.5 text-xs text-warning">
        Rate limited
      </span>
    );
  }
  return (
    <span className="inline-flex items-center rounded-full bg-success/10 px-2 py-0.5 text-xs text-success">
      Active
    </span>
  );
}
