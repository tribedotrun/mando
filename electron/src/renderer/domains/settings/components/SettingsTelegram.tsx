import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiGet } from '#renderer/domains/settings/hooks/useApi';
import { cardStyle, inputStyle, labelStyle, inputCls, labelCls } from '#renderer/styles';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import { toast } from 'sonner';
import type { TelegramConfig } from '#renderer/domains/settings/stores/settingsStore';
import { Switch } from '#renderer/global/components/Switch';

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
      style={{
        display: 'inline-block',
        width: 8,
        height: 8,
        borderRadius: '50%',
        backgroundColor: color,
        marginRight: 6,
        flexShrink: 0,
      }}
    />
  );
}

function RuntimeStatus({ health }: { health: TelegramHealth | undefined }): React.ReactElement {
  if (!health) {
    return (
      <span className="text-xs" style={{ color: 'var(--color-text-3)' }}>
        Loading...
      </span>
    );
  }
  if (!health.enabled) {
    return (
      <span className="text-xs flex items-center" style={{ color: 'var(--color-text-3)' }}>
        <StatusDot color="var(--color-text-3)" />
        Disabled
      </span>
    );
  }
  if (health.degraded) {
    return (
      <span className="text-xs flex items-center" style={{ color: 'var(--color-warning)' }}>
        <StatusDot color="var(--color-warning)" />
        Degraded{health.lastError ? ` — ${health.lastError}` : ''}
      </span>
    );
  }
  if (health.running) {
    return (
      <span className="text-xs flex items-center" style={{ color: 'var(--color-success)' }}>
        <StatusDot color="var(--color-success)" />
        Running
        {health.restartCount > 0 ? ` (${health.restartCount} restarts)` : ''}
      </span>
    );
  }
  return (
    <span className="text-xs flex items-center" style={{ color: 'var(--color-error)' }}>
      <StatusDot color="var(--color-error)" />
      Stopped{health.lastError ? ` — ${health.lastError}` : ''}
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

  const { data: health } = useQuery<TelegramHealth>({
    queryKey: ['health', 'telegram'],
    queryFn: () => apiGet<TelegramHealth>('/api/health/telegram'),
    refetchInterval: 10_000,
  });

  return (
    <div data-testid="settings-telegram" className="space-y-8">
      <h2 className="text-lg font-semibold text-text-1">Telegram</h2>

      <div className="space-y-6">
        {/* Runtime status */}
        <div style={cardStyle}>
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
              Status
            </span>
            <RuntimeStatus health={health} />
          </div>
        </div>

        {/* Enable toggle */}
        <div style={cardStyle}>
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-text-2">Enabled</h3>
            </div>
            <Switch
              testId="telegram-enabled"
              checked={!!telegram.enabled}
              onCheckedChange={async () => {
                const enabling = !telegram.enabled;
                updateTelegram({ enabled: enabling });
                const result = await save();
                if (!result.ok) {
                  updateTelegram({ enabled: !enabling });
                  toast.error(result.error ?? 'Failed to update Telegram settings');
                }
              }}
            />
          </div>
        </div>

        {/* Credentials */}
        <div style={cardStyle}>
          <h3 className="mb-4 text-sm font-medium text-text-2">Credentials</h3>
          <div className="space-y-4">
            <div>
              <label className={labelCls} style={labelStyle}>
                Bot Token
              </label>
              <input
                data-testid="telegram-bot-token"
                type="text"
                className={inputCls}
                style={inputStyle}
                value={botToken}
                onChange={(e) => {
                  updateEnv('TELEGRAM_MANDO_BOT_TOKEN', e.target.value);
                  scheduleSave();
                }}
                placeholder="123456:ABC-DEF..."
              />
            </div>
            <div>
              <label className={labelCls} style={labelStyle}>
                Owner
              </label>
              <p className="mb-1.5 text-xs text-text-3">
                Auto-detected when you /start the bot. Override here if needed.
              </p>
              <input
                data-testid="telegram-owner-id"
                className={inputCls}
                style={inputStyle}
                value={telegram.owner ?? ''}
                onChange={(e) => {
                  updateTelegram({ owner: e.target.value });
                  scheduleSave();
                }}
                placeholder="Auto-detected on first /start"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
