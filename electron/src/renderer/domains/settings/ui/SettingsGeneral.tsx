import React from 'react';
import { useNotificationsPref } from '#renderer/global/runtime/useDesktopNotifications';
import { Switch } from '#renderer/global/ui/switch';
import { useConfig, useLoginItemToggle } from '#renderer/domains/settings/runtime/hooks';
import {
  useUpdateLifecycle,
  UPDATE_CHANNELS,
} from '#renderer/domains/settings/runtime/useUpdateLifecycle';
import {
  SettingsRow,
  UpdateCheckButton,
  SegmentedControl,
} from '#renderer/domains/settings/ui/SettingsGeneralParts';

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
