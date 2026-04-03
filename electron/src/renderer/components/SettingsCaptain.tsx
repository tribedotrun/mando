import React from 'react';
import { cardStyle, inputStyle, labelStyle, inputCls, labelCls } from '#renderer/styles';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import type { CaptainConfig } from '#renderer/stores/settingsStore';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';

const EMPTY_CAPTAIN: CaptainConfig = {};

export function SettingsCaptain(): React.ReactElement {
  const captain = useSettingsStore((s) => s.config.captain ?? EMPTY_CAPTAIN);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

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

      <div style={cardStyle}>
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

      {/* Scheduling */}
      <div style={cardStyle}>
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
          Scheduling
        </h3>
        <p className="mt-0.5 mb-4 text-xs" style={{ color: 'var(--color-text-3)' }}>
          Controls how often captain ticks and when learning sessions run.
        </p>

        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className={labelCls} style={labelStyle}>
              Tick Interval (seconds)
            </label>
            <input
              data-testid="captain-tick-interval"
              type="number"
              min={5}
              max={3600}
              className={inputCls}
              style={inputStyle}
              value={captain.tickIntervalS ?? ''}
              onChange={(e) => {
                updateSection('captain', {
                  tickIntervalS: e.target.value ? Number(e.target.value) : undefined,
                });
                scheduleSave();
              }}
              placeholder="30"
            />
          </div>
          <div>
            <label className={labelCls} style={labelStyle}>
              Timezone
            </label>
            <input
              data-testid="captain-tz"
              className={inputCls}
              style={inputStyle}
              value={captain.tz ?? ''}
              onChange={(e) => {
                updateSection('captain', { tz: e.target.value });
                scheduleSave();
              }}
              placeholder="America/New_York"
            />
          </div>
        </div>
      </div>

      {/* Linear CLI */}
      <div style={cardStyle}>
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-2)' }}>
          Linear CLI
        </h3>
        <p className="mt-0.5 mb-4 text-xs" style={{ color: 'var(--color-text-3)' }}>
          Path to the Linear CLI binary used by captain for issue sync.
        </p>
        <div>
          <label className={labelCls} style={labelStyle}>
            CLI Path
          </label>
          <input
            data-testid="captain-linear-cli-path"
            className={inputCls}
            style={inputStyle}
            value={captain.linearCliPath ?? ''}
            onChange={(e) => {
              updateSection('captain', { linearCliPath: e.target.value });
              scheduleSave();
            }}
            placeholder="/usr/local/bin/linear"
          />
        </div>
      </div>
    </div>
  );
}
