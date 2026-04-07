import React from 'react';
import { cardStyle, inputStyle, labelStyle, inputCls, labelCls } from '#renderer/styles';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import { toast } from 'sonner';
import type { TelegramConfig } from '#renderer/domains/settings/stores/settingsStore';
import { Switch } from '#renderer/global/components/Switch';
import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';

const EMPTY_TELEGRAM: TelegramConfig = {};

export function SettingsTelegram(): React.ReactElement {
  const telegram = useSettingsStore((s) => s.config.channels?.telegram ?? EMPTY_TELEGRAM);
  const botToken = useSettingsStore((s) => s.config.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '');
  const updateTelegram = useSettingsStore((s) => s.updateTelegram);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

  return (
    <div data-testid="settings-telegram" className="space-y-8">
      <h2 className="text-lg font-semibold text-text-1">Telegram</h2>

      <div className="space-y-6">
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
                await save();
                if (!enabling) return;
                try {
                  await window.mandoAPI.launchd.reinstall();
                } catch (err) {
                  log.warn('[SettingsTelegram] launchd reinstall failed', err);
                  // Revert the toggle so the user sees the toggle reflect the
                  // actual system state (service not installed).
                  updateTelegram({ enabled: false });
                  await save();
                  toast.error(getErrorMessage(err, 'Failed to install Telegram service'));
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
