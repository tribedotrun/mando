import React from 'react';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import type { CaptainConfig } from '#renderer/stores/settingsStore';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

const EMPTY_CAPTAIN: CaptainConfig = {};

export function SettingsCaptain(): React.ReactElement {
  const captain = useSettingsStore((s) => s.config.captain ?? EMPTY_CAPTAIN);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);

  return (
    <div data-testid="settings-captain" className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
          Captain
        </h2>
        <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)' }}>
          Captain manages tasks, schedules work, and monitors workers.
        </p>
      </div>

      <div
        style={{
          borderRadius: 'var(--radius-panel)',
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface-1)',
          padding: '20px',
        }}
      >
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
              Auto Tick
            </h3>
            <p className="mt-0.5 text-xs" style={{ color: 'var(--color-text-3)' }}>
              Periodically tick the captain to schedule tasks and check worker health.
            </p>
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
