import React, { useState } from 'react';
import {
  useConfig,
  useConfigPatch,
  useTelegramHealth,
  type TelegramHealth,
} from '#renderer/domains/settings/runtime/hooks';
import { Card, CardContent } from '#renderer/global/ui/card';
import { Input } from '#renderer/global/ui/input';
import { Label } from '#renderer/global/ui/label';
import { Skeleton } from '#renderer/global/ui/skeleton';
import { telegramPatch, envPatch } from '#renderer/global/service/configPatches';
import type { TelegramConfig } from '#renderer/global/types';
import { toast } from 'sonner';
import { Switch } from '#renderer/global/ui/switch';

function StatusDot({ color }: { color: string }): React.ReactElement {
  return (
    <span
      className="mr-1.5 inline-block size-2 shrink-0 rounded-full"
      style={{ backgroundColor: color }}
    />
  );
}

function RuntimeStatus({ health }: { health: TelegramHealth | undefined }): React.ReactElement {
  if (!health) {
    return <Skeleton className="h-4 w-16" />;
  }
  if (!health.enabled) {
    return (
      <span className="flex items-center text-xs text-muted-foreground">
        <StatusDot color="var(--muted-foreground)" />
        Disabled
      </span>
    );
  }
  if (health.degraded) {
    return (
      <span className="flex items-center text-xs text-warning">
        <StatusDot color="var(--warning)" />
        Degraded{health.lastError ? ` \u2014 ${health.lastError}` : ''}
      </span>
    );
  }
  if (health.running) {
    return (
      <span className="flex items-center text-xs text-success">
        <StatusDot color="var(--success)" />
        Running
        {health.restartCount > 0 ? ` (${health.restartCount} restarts)` : ''}
      </span>
    );
  }
  return (
    <span className="flex items-center text-xs text-destructive">
      <StatusDot color="var(--destructive)" />
      Stopped{health.lastError ? ` \u2014 ${health.lastError}` : ''}
    </span>
  );
}

const EMPTY_TELEGRAM: TelegramConfig = Object.freeze({});

export function SettingsTelegram(): React.ReactElement {
  const { data: config } = useConfig();
  const { save, debouncedSave } = useConfigPatch();
  const telegram = config?.channels?.telegram ?? EMPTY_TELEGRAM;
  const botToken = config?.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '';

  const [savingEnabled, setSavingEnabled] = useState(false);
  const { data: health } = useTelegramHealth();

  return (
    <div data-testid="settings-telegram" className="space-y-8">
      <h2 className="text-lg font-semibold text-foreground">Telegram</h2>

      <div className="space-y-6">
        {/* Runtime status */}
        <Card className="py-4">
          <CardContent>
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium text-muted-foreground">Status</span>
              <RuntimeStatus health={health} />
            </div>
          </CardContent>
        </Card>

        {/* Enable toggle */}
        <Card className="py-4">
          <CardContent>
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-sm font-medium text-muted-foreground">Enabled</h3>
              </div>
              <Switch
                data-testid="telegram-enabled"
                checked={!!telegram.enabled}
                disabled={savingEnabled}
                onCheckedChange={() => {
                  setSavingEnabled(true);
                  const enabling = !telegram.enabled;
                  save(telegramPatch({ enabled: enabling }), {
                    onError: () => toast.error('Failed to update Telegram settings'),
                    onSettled: () => setSavingEnabled(false),
                  });
                }}
              />
            </div>
          </CardContent>
        </Card>

        {/* Credentials */}
        <Card className="py-4">
          <CardContent>
            <h3 className="mb-4 text-sm font-medium text-muted-foreground">Credentials</h3>
            <div className="space-y-4">
              <div>
                <Label className="mb-1.5 text-xs text-muted-foreground">Bot Token</Label>
                <Input
                  data-testid="telegram-bot-token"
                  type="text"
                  value={botToken}
                  onChange={(e) => {
                    debouncedSave(envPatch({ TELEGRAM_MANDO_BOT_TOKEN: e.target.value }));
                  }}
                  placeholder="123456:ABC-DEF..."
                />
              </div>
              <div>
                <Label className="mb-1.5 text-xs text-muted-foreground">Owner</Label>
                <p className="mb-1.5 text-xs text-muted-foreground">
                  Auto-detected when you /start the bot. Override here if needed.
                </p>
                <Input
                  data-testid="telegram-owner-id"
                  value={telegram.owner ?? ''}
                  onChange={(e) => {
                    debouncedSave(telegramPatch({ owner: e.target.value }));
                  }}
                  placeholder="Auto-detected on first /start"
                />
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
