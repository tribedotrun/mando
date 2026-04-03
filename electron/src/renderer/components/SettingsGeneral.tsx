import React, { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import log from '#renderer/logger';
import { fetchHealth } from '#renderer/api';
import {
  getNotificationsEnabled,
  setNotificationsEnabled,
} from '#renderer/hooks/useDesktopNotifications';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useToastStore } from '#renderer/stores/toastStore';
import { useSettingsStore } from '#renderer/stores/settingsStore';

const CHANNELS = ['stable', 'beta'] as const;

type ConnectionState = 'connected' | 'connecting' | 'disconnected';

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

function SectionDivider() {
  return <div style={{ height: 1, background: 'var(--color-border-subtle)', margin: '8px 0' }} />;
}

const GATEWAY_STATE: Record<string, { dot: string; label: string }> = {
  connected: { dot: 'var(--color-success)', label: 'Connected' },
  connecting: { dot: 'var(--color-stale)', label: 'Connecting' },
  disconnected: { dot: 'var(--color-error)', label: 'Disconnected' },
  updating: { dot: 'var(--color-stale)', label: 'Updating' },
};
const GATEWAY_FALLBACK = GATEWAY_STATE.disconnected;

export function SettingsGeneral(): React.ReactElement {
  const [channelOverride, setChannelOverride] = useState<string | null>(null);
  const [notificationsEnabled, setNotifState] = useState(getNotificationsEnabled);
  const [liveConnectionState, setLiveConnectionState] = useState<ConnectionState | null>(null);
  const startAtLogin = useSettingsStore((s) => s.config.startAtLogin ?? false);
  const update = useSettingsStore((s) => s.update);
  const save = useSettingsStore((s) => s.save);

  const { data: systemInfo } = useQuery({
    queryKey: ['settings', 'general', 'systemInfo'],
    queryFn: async () => {
      if (!window.mandoAPI) {
        return {
          dataDir: '',
          configPath: '',
          appVersion: '',
          channel: 'stable',
          gatewayUrl: '',
          connectionState: 'disconnected' as ConnectionState,
        };
      }
      const [dataDir, configPath, appVersion, channel, gatewayUrl, currentConnectionState] =
        await Promise.all([
          window.mandoAPI.dataDir(),
          window.mandoAPI.configPath(),
          window.mandoAPI.updates.appVersion(),
          window.mandoAPI.updates.getChannel(),
          window.mandoAPI.gatewayUrl(),
          window.mandoAPI.connectionState(),
        ]);
      return {
        dataDir,
        configPath,
        appVersion,
        channel,
        gatewayUrl,
        connectionState: currentConnectionState as ConnectionState,
      };
    },
  });
  const { data: health } = useQuery({
    queryKey: ['settings', 'general', 'health'],
    queryFn: fetchHealth,
    retry: false,
  });

  useMountEffect(() => {
    if (!window.mandoAPI) return;
    window.mandoAPI.onConnectionState((state) => {
      setLiveConnectionState(state as ConnectionState);
    });
    return () => {
      window.mandoAPI.removeConnectionStateListeners();
    };
  });

  const dataDir = systemInfo?.dataDir ?? '';
  const configPath = systemInfo?.configPath ?? '';
  const appVersion = systemInfo?.appVersion ?? '';
  const updateChannel = channelOverride ?? systemInfo?.channel ?? 'stable';
  const gatewayUrl = systemInfo?.gatewayUrl ?? '';
  const connectionState = liveConnectionState ?? systemInfo?.connectionState ?? 'connecting';

  const gatewayDisplay = useMemo(() => {
    if (!gatewayUrl) return 'Gateway unavailable';
    try {
      const url = new URL(gatewayUrl);
      return `${url.hostname}:${url.port}`;
    } catch {
      return gatewayUrl;
    }
  }, [gatewayUrl]);

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

      <div className="text-label" style={{ color: 'var(--color-accent)', marginBottom: 12 }}>
        Application
      </div>

      <SettingsRow label="Version">
        <span className="text-code" style={{ color: 'var(--color-text-1)' }}>
          {appVersion || '\u2014'}
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
      <p className="text-caption" style={{ color: 'var(--color-text-3)', marginTop: -4 }}>
        Launch Mando when you log in to your Mac
      </p>

      <SettingsRow label="Desktop notifications">
        <ToggleSwitch
          testId="notifications-toggle"
          checked={notificationsEnabled}
          onChange={toggleNotifications}
        />
      </SettingsRow>
      <p className="text-caption" style={{ color: 'var(--color-text-3)', marginTop: -4 }}>
        Show macOS notifications for agent events
      </p>

      <SectionDivider />

      <div
        className="text-label"
        style={{ color: 'var(--color-accent)', marginBottom: 12, marginTop: 16 }}
      >
        System
      </div>

      <SettingsRow label="Data directory">
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {dataDir || '~/.mando/'}
        </span>
      </SettingsRow>

      <SettingsRow label="Config file">
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {configPath || '~/.mando/config.json'}
        </span>
      </SettingsRow>

      <SettingsRow label="Task database">
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {health?.taskDbPath || '~/.mando/mando.db'}
        </span>
      </SettingsRow>

      <SettingsRow label="Worker health">
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {health?.workerHealthPath || '~/.mando/state/worker-health.json'}
        </span>
      </SettingsRow>

      <SettingsRow label="Captain lock">
        <span className="text-code" style={{ color: 'var(--color-text-2)' }}>
          {health?.lockfilePath || '~/.mando/captain.lock'}
        </span>
      </SettingsRow>

      <SettingsRow label="Gateway">
        <span className="flex items-center gap-2">
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: 3,
              background: (GATEWAY_STATE[connectionState] ?? GATEWAY_FALLBACK).dot,
              flexShrink: 0,
            }}
          />
          <span className="text-code" style={{ color: 'var(--color-text-1)' }}>
            {gatewayDisplay}
          </span>
          <span className="text-caption" style={{ color: 'var(--color-text-3)' }}>
            {(GATEWAY_STATE[connectionState] ?? GATEWAY_FALLBACK).label}
          </span>
        </span>
      </SettingsRow>

      {health?.restartRequired ? (
        <p className="text-caption" style={{ color: 'var(--color-stale)', marginTop: 8 }}>
          Path changes are saved, but the daemon will keep using the active runtime paths until it
          restarts.
        </p>
      ) : null}
    </div>
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
