import React, { useState } from 'react';
import log from '#renderer/global/service/logger';
import { useNotificationsPref } from '#renderer/global/runtime/useDesktopNotifications';
import { Switch } from '#renderer/global/ui/switch';
import { Button } from '#renderer/global/ui/button';
import { useConfig, useLoginItemToggle } from '#renderer/domains/settings/runtime/hooks';
import {
  useUpdateLifecycle,
  UPDATE_CHANNELS,
  type UpdateCheckStatus,
} from '#renderer/domains/settings/runtime/useUpdateLifecycle';

function SettingsRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex min-h-[40px] items-center justify-between py-2.5">
      <span className="text-body text-foreground">{label}</span>
      <div className="flex items-center">{children}</div>
    </div>
  );
}

export function SettingsGeneral(): React.ReactElement {
  const { enabled: notificationsEnabled, toggle: toggleNotifications } = useNotificationsPref();
  const { data: config } = useConfig();
  const openAtLogin = config?.ui?.openAtLogin ?? false;

  const {
    appVersion,
    updateChannel,
    updateCheckStatus,
    savingChannel,
    changeChannel,
    checkForUpdates,
    installUpdate,
    setLoginItem,
    onInstallError,
  } = useUpdateLifecycle();

  const { toggle: toggleLoginItem, saving: savingLoginItem } = useLoginItemToggle(setLoginItem);

  return (
    <div data-testid="settings-general">
      <h2 className="mb-6 text-heading text-foreground">General</h2>

      <SettingsRow label="Version">
        <span className="flex items-center gap-3">
          <span className="text-code text-foreground">{appVersion || '\u2014'}</span>
          <UpdateCheckButton
            status={updateCheckStatus}
            onCheck={checkForUpdates}
            onInstall={installUpdate}
            onInstallError={onInstallError}
          />
        </span>
      </SettingsRow>

      <SettingsRow label="Update channel">
        <SegmentedControl
          options={UPDATE_CHANNELS}
          value={updateChannel}
          onChange={changeChannel}
          disabled={savingChannel}
        />
      </SettingsRow>

      <SettingsRow label="Open app at login">
        <Switch
          data-testid="start-at-login-toggle"
          checked={openAtLogin}
          onCheckedChange={() => toggleLoginItem(openAtLogin)}
          disabled={savingLoginItem}
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
  onCheck,
  onInstall,
  onInstallError,
}: {
  status: UpdateCheckStatus;
  onCheck: () => void;
  onInstall: () => Promise<void>;
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
          onInstall()
            .catch((err: unknown) => {
              log.error('[Settings] install update failed:', err);
              onInstallError();
            })
            .finally(() => setInstalling(false));
        }}
        className={`text-caption text-muted-foreground ${installing ? 'opacity-60' : ''}`}
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
      onClick={onCheck}
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
  disabled,
}: {
  options: readonly string[];
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
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
            disabled={disabled}
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
