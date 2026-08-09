import React from 'react';
import { Monitor } from 'lucide-react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Button } from '#renderer/global/ui/primitives/button';
import { useCodexDesktopAppStatus } from '#renderer/domains/settings/runtime/useFeedbackCodexDesktopApp';

/** Nothing to show when the desktop app is on the personal/ambient account. */
export function CodexDesktopAppBanner(): React.ReactElement | null {
  const { status, restorePersonal, restoring, anySwapInFlight } = useCodexDesktopAppStatus();

  if (!status || status.mode !== 'pool') return null;

  return (
    <Card className="border border-warning/30 bg-warning/5 py-3">
      <CardContent className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-sm text-foreground">
          <Monitor size={14} className="shrink-0 text-warning" />
          <span>
            Desktop app is using{' '}
            <span className="font-medium">{status.activeLabel ?? 'an unknown account'}</span>
          </span>
        </div>
        {status.canRestore ? (
          <Button variant="outline" size="sm" disabled={anySwapInFlight} onClick={restorePersonal}>
            {restoring ? 'Restoring…' : 'Restore personal'}
          </Button>
        ) : null}
      </CardContent>
    </Card>
  );
}
