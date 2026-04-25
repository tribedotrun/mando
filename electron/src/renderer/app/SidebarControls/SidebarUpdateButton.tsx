import React from 'react';
import { useUpdateBanner } from '#renderer/global/runtime/useNativeActions';
import { Button } from '#renderer/global/ui/primitives/button';

export function SidebarUpdateButton(): React.ReactElement | null {
  const { updateReady, installing, installUpdate } = useUpdateBanner();
  if (!updateReady) return null;

  return (
    <Button
      size="xs"
      disabled={installing}
      onClick={installUpdate}
      className="absolute right-3 top-3 z-20"
      style={{ WebkitAppRegion: 'no-drag' }}
    >
      {installing ? 'Installing…' : 'Update'}
    </Button>
  );
}
