import React from 'react';
import { cardStyle } from '#renderer/styles';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import type { CaptainConfig } from '#renderer/domains/settings/stores/settingsStore';
import { Switch } from '#renderer/global/components/Switch';

const EMPTY_CAPTAIN: CaptainConfig = {};

export function SettingsCaptain(): React.ReactElement {
  const captain = useSettingsStore((s) => s.config.captain ?? EMPTY_CAPTAIN);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);

  return (
    <div data-testid="settings-captain">
      <h2 className="text-heading text-text-1" style={{ marginBottom: 4 }}>
        Captain
      </h2>
      <p className="text-caption text-text-3" style={{ marginBottom: 24 }}>
        Ticks every 30 seconds to check task progress, review PRs, and intervene when needed.
      </p>

      <div style={cardStyle}>
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-medium text-text-2">Auto Tick</h3>
          </div>
          <Switch
            testId="captain-auto-tick"
            checked={!!captain.autoSchedule}
            onCheckedChange={(checked) => {
              updateSection('captain', { autoSchedule: checked });
              save();
            }}
          />
        </div>
      </div>
    </div>
  );
}
