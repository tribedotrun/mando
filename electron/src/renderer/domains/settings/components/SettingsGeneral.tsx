import React, { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import log from '#renderer/logger';
import {
  getNotificationsEnabled,
  setNotificationsEnabled,
} from '#renderer/global/hooks/useDesktopNotifications';
import { Switch } from '#renderer/components/ui/switch';
import { Button } from '#renderer/components/ui/button';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { toast } from 'sonner';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';

const CHANNELS = ['stable', 'beta'] as const;

function SettingsRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex min-h-[40px] items-center justify-between py-2.5">
      <span className="text-body text-foreground">{label}</span>
      <div className="flex items-center">{children}</div>
    </div>
  );
}

type UpdateCheckStatus =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'update-available'
  | 'error'
  | 'install-error';

export function SettingsGeneral(): React.ReactElement {
  const [channelOverride, setChannelOverride] = useState<string | null>(null);
  const [notificationsEnabled, setNotifState] = useState(getNotificationsEnabled);
  const [updateCheckStatus, setUpdateCheckStatus] = useState<UpdateCheckStatus>('idle');
  const clearTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const openAtLogin = useSettingsStore((s) => s.config.ui?.openAtLogin ?? false);
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
      toast.error('Failed to change update channel');
    }
  };

  const toggleNotifications = () => {
    const next = !notificationsEnabled;
    setNotificationsEnabled(next);
    setNotifState(next);
  };

  const toggleLoginItem = async () => {
    const next = !openAtLogin;
    update({ ui: { ...(useSettingsStore.getState().config.ui || {}), openAtLogin: next } });
    const result = await save();
    if (!result.ok) {
      log.warn('[SettingsGeneral] login-item save failed:', result.error);
      update({ ui: { ...(useSettingsStore.getState().config.ui || {}), openAtLogin: !next } });
      toast.error(result.error ?? 'Failed to save login setting');
      return;
    }
    try {
      await window.mandoAPI.setLoginItem(next);
    } catch (err) {
      log.error('[SettingsGeneral] login item IPC failed:', err);
      update({ ui: { ...(useSettingsStore.getState().config.ui || {}), openAtLogin: !next } });
      toast.error('Failed to change login setting');
    }
  };

  return (
    <div data-testid="settings-general">
      <h2 className="mb-6 text-heading text-foreground">General</h2>

      <SettingsRow label="Version">
        <span className="flex items-center gap-3">
          <span className="text-code text-foreground">{appVersion || '\u2014'}</span>
          <UpdateCheckButton
            status={updateCheckStatus}
            onCheckError={() => setUpdateCheckStatus('error')}
            onInstallError={() => {
              setUpdateCheckStatus('install-error');
              clearTimerRef.current = setTimeout(() => setUpdateCheckStatus('idle'), 4000);
            }}
          />
        </span>
      </SettingsRow>

      <SettingsRow label="Update channel">
        <SegmentedControl options={CHANNELS} value={updateChannel} onChange={handleChannelChange} />
      </SettingsRow>

      <SettingsRow label="Open app at login">
        <Switch
          data-testid="start-at-login-toggle"
          checked={openAtLogin}
          onCheckedChange={toggleLoginItem}
        />
      </SettingsRow>

      <SettingsRow label="Desktop notifications">
        <Switch
          data-testid="notifications-toggle"
          checked={notificationsEnabled}
          onCheckedChange={toggleNotifications}
        />
      </SettingsRow>
    </div>
  );
}

function UpdateCheckButton({
  status,
  onCheckError,
  onInstallError,
}: {
  status: UpdateCheckStatus;
  onCheckError: () => void;
  onInstallError: () => void;
}) {
  const [installing, setInstalling] = useState(false);

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
        onClick={() => {
          setInstalling(true);
          window.mandoAPI.updates
            .installUpdate()
            .catch((err: unknown) => {
              log.error('[Settings] install update failed:', err);
              onInstallError();
            })
            .finally(() => setInstalling(false));
        }}
        className={`text-caption text-primary ${installing ? 'opacity-60' : ''}`}
      >
        {installing ? 'Installing...' : 'Update ready \u2014 install'}
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
      onClick={() => {
        window.mandoAPI.updates.checkForUpdates().catch(onCheckError);
      }}
      className="text-caption text-muted-foreground hover:text-foreground"
    >
      Check for updates
    </Button>
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
    <div data-testid="update-channel-select" className="flex overflow-hidden rounded-md bg-muted">
      {options.map((opt) => {
        const active = value === opt;
        return (
          <Button
            key={opt}
            variant="ghost"
            size="sm"
            onClick={() => onChange(opt)}
            className={`h-auto rounded-none px-4 py-1 text-[13px] transition-colors ${
              active
                ? 'bg-secondary font-medium text-foreground'
                : 'bg-transparent font-normal text-muted-foreground'
            }`}
          >
            {opt.charAt(0).toUpperCase() + opt.slice(1)}
          </Button>
        );
      })}
    </div>
  );
}
