import React from 'react';
import { cardStyle } from '#renderer/styles';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import type { CaptainConfig } from '#renderer/stores/settingsStore';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

const EMPTY_CAPTAIN: CaptainConfig = {};

export function SettingsCaptain(): React.ReactElement {
  const captain = useSettingsStore((s) => s.config.captain ?? EMPTY_CAPTAIN);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);

  return (
    <div data-testid="settings-captain">
      <h2 className="text-heading" style={{ color: 'var(--color-text-1)', marginBottom: 4 }}>
        Captain
      </h2>
      <p className="text-caption" style={{ color: 'var(--color-text-3)', marginBottom: 24 }}>
        Ticks every 30 seconds to check task progress, review PRs, and intervene when needed.
      </p>

      <div style={cardStyle}>
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
              Auto Tick
            </h3>
          </div>
          <ToggleSwitch
            testId="captain-auto-tick"
            checked={!!captain.autoSchedule}
            onChange={() => {
              updateSection('captain', { autoSchedule: !captain.autoSchedule });
              save();
            }}
          />
        </div>
      </div>
    </div>
  );
}
