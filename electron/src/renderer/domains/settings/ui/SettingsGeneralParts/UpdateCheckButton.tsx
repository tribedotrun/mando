import React, { useState } from 'react';
import log from '#renderer/global/service/logger';
import { Button } from '#renderer/global/ui/primitives/button';
import type { UpdateCheckStatus } from '#renderer/domains/settings/runtime/useUpdateLifecycle';

export function UpdateCheckButton({
  status,
  onCheck,
  onInstall,
  onInstallError,
}: {
  status: UpdateCheckStatus;
  onCheck: () => void;
  onInstall: () => Promise<void>;
  onInstallError: () => void;
}): React.ReactElement | null {
  const [installing, setInstalling] = useState(false);

  const install = async (): Promise<void> => {
    setInstalling(true);
    try {
      await onInstall();
    } catch (err: unknown) {
      log.error('[Settings] install update failed:', err);
      onInstallError();
    } finally {
      setInstalling(false);
    }
  };

  if (status === 'checking') {
    return <span className="text-caption text-muted-foreground">Checking...</span>;
  }
  if (status === 'up-to-date') {
    return <span className="text-caption text-success">Up to date</span>;
  }
  if (status === 'update-available') {
    return (
      <Button
        variant="link"
        size="xs"
        disabled={installing}
        onClick={() => void install()}
        className={`text-caption text-muted-foreground ${installing ? 'opacity-60' : ''}`}
      >
        {installing ? 'Installing...' : 'Update ready — install'}
      </Button>
    );
  }
  if (status === 'error') {
    return <span className="text-caption text-destructive">Check failed</span>;
  }
  if (status === 'install-error') {
    return <span className="text-caption text-destructive">Install failed</span>;
  }
  return (
    <Button
      variant="link"
      size="xs"
      onClick={onCheck}
      className="text-caption text-muted-foreground hover:text-foreground"
    >
      Check for updates
    </Button>
  );
}
