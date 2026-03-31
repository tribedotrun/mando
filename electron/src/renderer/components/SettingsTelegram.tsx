import React from 'react';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useToastStore } from '#renderer/stores/toastStore';
import type { TelegramConfig } from '#renderer/stores/settingsStore';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

const EMPTY_TELEGRAM: TelegramConfig = {};

const cardStyle = {
  borderRadius: 'var(--radius-panel)',
  border: '1px solid var(--color-border)',
  background: 'var(--color-surface-1)',
  padding: '20px',
};

const inputStyle: React.CSSProperties = {
  border: '1px solid var(--color-border)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
};

const labelStyle = { color: 'var(--color-text-3)' };

export function SettingsTelegram(): React.ReactElement {
  const telegram = useSettingsStore((s) => s.config.channels?.telegram ?? EMPTY_TELEGRAM);
  const botToken = useSettingsStore((s) => s.config.env?.TELEGRAM_MANDO_BOT_TOKEN ?? '');
  const updateTelegram = useSettingsStore((s) => s.updateTelegram);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

  const inputCls =
    'w-full rounded-md px-3 py-2 text-sm placeholder-[var(--color-text-3)] focus:outline-none focus:ring-1';
  const labelCls = 'mb-1 block text-xs font-medium uppercase tracking-wider';

  return (
    <div data-testid="settings-telegram" className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
          Telegram
        </h2>
        <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)' }}>
          Configure the Telegram bot for notifications and commands.
        </p>
      </div>

      <div className="space-y-6">
        {/* Enable toggle */}
        <div style={cardStyle}>
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
                Enabled
              </h3>
              <p className="mt-0.5 text-xs" style={{ color: 'var(--color-text-3)' }}>
                Turn the Telegram bot on or off.
              </p>
            </div>
            <ToggleSwitch
              testId="telegram-enabled"
              checked={!!telegram.enabled}
              onChange={() => {
                const enabling = !telegram.enabled;
                updateTelegram({ enabled: enabling });
                save();
                if (enabling)
                  window.mandoAPI.launchd.reinstall().catch(() => {
                    useToastStore.getState().add('error', 'Failed to install Telegram service');
                  });
              }}
            />
          </div>
        </div>

        {/* Credentials */}
        <div style={cardStyle}>
          <h3 className="mb-4 text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
            Credentials
          </h3>
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
              <p className="mb-1.5 text-xs" style={{ color: 'var(--color-text-3)' }}>
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
