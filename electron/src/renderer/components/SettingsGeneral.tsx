import React, { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import log from '#renderer/logger';
import {
  getNotificationsEnabled,
  setNotificationsEnabled,
} from '#renderer/hooks/useDesktopNotifications';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useToastStore } from '#renderer/stores/toastStore';
import { useSettingsStore } from '#renderer/stores/settingsStore';

const CHANNELS = ['stable', 'beta'] as const;

function SettingsRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between" style={{ padding: '10px 0', minHeight: 40 }}>
      <span className="text-body" style={{ color: 'var(--color-text-1)' }}>
        {label}
      </span>
      <div className="flex items-center">{children}</div>
    </div>
  );
}

type UpdateCheckStatus = 'idle' | 'checking' | 'up-to-date' | 'update-available' | 'error';

export function SettingsGeneral(): React.ReactElement {
  const [channelOverride, setChannelOverride] = useState<string | null>(null);
  const [notificationsEnabled, setNotifState] = useState(getNotificationsEnabled);
  const [updateCheckStatus, setUpdateCheckStatus] = useState<UpdateCheckStatus>('idle');
  const clearTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const startAtLogin = useSettingsStore((s) => s.config.startAtLogin ?? false);
  const update = useSettingsStore((s) => s.update);
  const save = useSettingsStore((s) => s.save);

  const { data: systemInfo } = useQuery({
    queryKey: ['settings', 'general', 'systemInfo'],
    queryFn: async () => {
      if (!window.mandoAPI) {
        return { appVersion: '', channel: 'stable' };
      }
      const [appVersion, channel] = await Promise.all([
        window.mandoAPI.updates.appVersion(),
        window.mandoAPI.updates.getChannel(),
      ]);
      return { appVersion, channel };
    },
  });

  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.updates.onUpdateChecking(() => {
      clearTimeout(clearTimerRef.current);
      setUpdateCheckStatus('checking');
    });
    window.mandoAPI.updates.onUpdateNoUpdate(() => {
      setUpdateCheckStatus('up-to-date');
      clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), 4000);
    });
    window.mandoAPI.updates.onUpdateCheckError(() => {
      setUpdateCheckStatus('error');
      clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), 4000);
    });
    window.mandoAPI.updates.onUpdateCheckDone(({ found }) => {
      if (found) setUpdateCheckStatus('update-available');
    });
    return () => {
      clearTimeout(clearTimerRef.current);
      window.mandoAPI.updates.removeCheckListeners();
    };
  });

  const appVersion = systemInfo?.appVersion ?? '';
  const updateChannel = channelOverride ?? systemInfo?.channel ?? 'stable';

  const handleChannelChange = async (channel: string) => {
    setChannelOverride(channel);
    try {
      await window.mandoAPI.updates.setChannel(channel);
    } catch (err) {
      log.error('[SettingsGeneral] channel change failed:', err);
      setChannelOverride(null);
      useToastStore.getState().add('error', 'Failed to change update channel');
    }
  };

  const toggleNotifications = () => {
    const next = !notificationsEnabled;
    setNotificationsEnabled(next);
    setNotifState(next);
  };

  const toggleLoginItem = async () => {
    const next = !startAtLogin;
    update({ startAtLogin: next });
    await save();
    if (useSettingsStore.getState().error) {
      update({ startAtLogin: !next });
      useToastStore.getState().add('error', 'Failed to save login setting');
      return;
    }
    try {
      await window.mandoAPI.setLoginItem(next);
    } catch (err) {
      log.error('[SettingsGeneral] login item IPC failed:', err);
      update({ startAtLogin: !next });
      useToastStore.getState().add('error', 'Failed to change login setting');
    }
  };

  return (
    <div data-testid="settings-general">
      <h2 className="text-heading" style={{ color: 'var(--color-text-1)', marginBottom: 24 }}>
        General
      </h2>

      <SettingsRow label="Version">
        <span className="flex items-center gap-3">
          <span className="text-code" style={{ color: 'var(--color-text-1)' }}>
            {appVersion || '\u2014'}
          </span>
          <UpdateCheckButton
            status={updateCheckStatus}
            onError={() => setUpdateCheckStatus('error')}
          />
        </span>
      </SettingsRow>

      <SettingsRow label="Update channel">
        <SegmentedControl options={CHANNELS} value={updateChannel} onChange={handleChannelChange} />
      </SettingsRow>

      <SettingsRow label="Start at login">
        <ToggleSwitch
          testId="start-at-login-toggle"
          checked={startAtLogin}
          onChange={toggleLoginItem}
        />
      </SettingsRow>

      <SettingsRow label="Desktop notifications">
        <ToggleSwitch
          testId="notifications-toggle"
          checked={notificationsEnabled}
          onChange={toggleNotifications}
        />
      </SettingsRow>
    </div>
  );
}

function UpdateCheckButton({
  status,
  onError,
}: {
  status: UpdateCheckStatus;
  onError: () => void;
}) {
  if (status === 'checking') {
    return (
      <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
        Checking…
      </span>
    );
  }
  if (status === 'up-to-date') {
    return (
      <span className="text-caption" style={{ color: 'var(--color-success)' }}>
        Up to date
      </span>
    );
  }
  if (status === 'update-available') {
    return (
      <span className="text-caption" style={{ color: 'var(--color-accent)' }}>
        Update ready
      </span>
    );
  }
  if (status === 'error') {
    return (
      <span className="text-caption" style={{ color: 'var(--color-error)' }}>
        Check failed
      </span>
    );
  }
  return (
    <button
      onClick={() => {
        window.mandoAPI.updates.checkForUpdates().catch(onError);
      }}
      className="text-caption transition-colors"
      style={{
        color: 'var(--color-text-3)',
        background: 'none',
        border: 'none',
        cursor: 'pointer',
        padding: 0,
        textDecoration: 'underline',
        textUnderlineOffset: 2,
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.color = 'var(--color-text-1)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.color = 'var(--color-text-3)';
      }}
    >
      Check for updates
    </button>
  );
}

function SegmentedControl({
  options,
  value,
  onChange,
}: {
  options: readonly string[];
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div
      data-testid="update-channel-select"
      className="flex"
      style={{
        borderRadius: 'var(--radius-button)',
        border: '1px solid var(--color-border)',
        overflow: 'hidden',
      }}
    >
      {options.map((opt, index) => {
        const active = value === opt;
        return (
          <button
            key={opt}
            onClick={() => onChange(opt)}
            className="text-[13px] transition-colors"
            style={{
              padding: '5px 16px',
              background: active ? 'var(--color-surface-3)' : 'transparent',
              color: active ? 'var(--color-text-1)' : 'var(--color-text-2)',
              fontWeight: active ? 500 : 400,
              border: 'none',
              borderRight: index === options.length - 1 ? 'none' : '1px solid var(--color-border)',
              cursor: 'pointer',
            }}
          >
            {opt.charAt(0).toUpperCase() + opt.slice(1)}
          </button>
        );
      })}
    </div>
  );
}
