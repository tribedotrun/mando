import React from 'react';
import { useNotificationsPref } from '#renderer/global/runtime/useDesktopNotifications';
import { Switch } from '#renderer/global/ui/primitives/switch';
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

  const lifecycle = useUpdateLifecycle();

  const { toggle: toggleLoginItem, saving: savingLoginItem } = useLoginItemToggle(
    lifecycle.login.setLoginItem,
  );

  return (
    <div data-testid="settings-general">
      <h2 className="mb-6 text-heading text-foreground">General</h2>

      <SettingsRow label="Version">
        <span className="flex items-center gap-3">
          <span className="text-code text-foreground">{lifecycle.app.version || '\u2014'}</span>
          <UpdateCheckButton
            status={lifecycle.update.status}
            onCheck={lifecycle.update.check}
            onInstall={lifecycle.update.install}
            onInstallError={lifecycle.events.onInstallError}
          />
        </span>
      </SettingsRow>

      <SettingsRow label="Update channel">
        <SegmentedControl
          options={UPDATE_CHANNELS}
          value={lifecycle.channel.value}
          onChange={lifecycle.channel.change}
          disabled={lifecycle.channel.saving}
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
