import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiGet } from '#renderer/domains/settings/hooks/useApi';
import { Card, CardContent } from '#renderer/components/ui/card';
import { Input } from '#renderer/components/ui/input';
import { Label } from '#renderer/components/ui/label';
import { Skeleton } from '#renderer/components/ui/skeleton';
import {
  useSettingsStore,
  type TelegramConfig,
} from '#renderer/domains/settings/stores/settingsStore';
import { toast } from 'sonner';
import { Switch } from '#renderer/components/ui/switch';

interface TelegramHealth {
  enabled: boolean;
  running: boolean;
  owner: string;
  lastError: string | null;
  degraded: boolean;
  restartCount: number;
  mode: string;
}

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

const EMPTY_TELEGRAM: TelegramConfig = {};

export function SettingsTelegram(): React.ReactElement {
  const telegram = useSettingsStore((s) => s.config.channels?.telegram ?? EMPTY_TELEGRAM);
  const botToken = useSettingsStore((s) => s.config.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '');
  const updateTelegram = useSettingsStore((s) => s.updateTelegram);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

  const [savingEnabled, setSavingEnabled] = useState(false);

  const { data: health } = useQuery<TelegramHealth>({
    queryKey: ['health', 'telegram'],
    queryFn: () => apiGet<TelegramHealth>('/api/health/telegram'),
    refetchInterval: 10_000,
  });

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
                  updateTelegram({ enabled: enabling });
                  void save()
                    .then((result) => {
                      if (!result.ok) {
                        updateTelegram({ enabled: !enabling });
                        toast.error(result.error ?? 'Failed to update Telegram settings');
                      }
                    })
                    .catch(() => {
                      updateTelegram({ enabled: !enabling });
                      toast.error('Failed to update Telegram settings');
                    })
                    .finally(() => setSavingEnabled(false));
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
                    updateEnv('TELEGRAM_MANDO_BOT_TOKEN', e.target.value);
                    scheduleSave();
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
                    updateTelegram({ owner: e.target.value });
                    scheduleSave();
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
