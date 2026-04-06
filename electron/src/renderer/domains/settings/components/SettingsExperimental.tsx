import React from 'react';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import type { FeaturesConfig } from '#renderer/domains/settings/stores/settingsStore';
import { ToggleSwitch } from '#renderer/global/components/ToggleSwitch';

const EMPTY_FEATURES: FeaturesConfig = {};

interface FlagDef {
  key: keyof FeaturesConfig;
  label: string;
  description: string;
}

const FLAGS: FlagDef[] = [
  {
    key: 'scout',
    label: 'Scout',
    description: 'Research tech blogs and turn them into actionable tasks for your project.',
  },
];

export function SettingsExperimental(): React.ReactElement {
  const features = useSettingsStore((s) => s.config.features ?? EMPTY_FEATURES);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);

  return (
    <div data-testid="settings-experimental">
      <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
        Experimental
      </h2>
      <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)', marginBottom: 24 }}>
        Alpha features. These may change or be removed at any time.
      </p>

      <div
        style={{
          borderRadius: 'var(--radius-panel)',
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface-1)',
          overflow: 'hidden',
        }}
      >
        {FLAGS.map((flag, i) => {
          const on = !!features[flag.key];
          return (
            <div
              key={flag.key}
              style={i > 0 ? { borderTop: '1px solid var(--color-border-subtle)' } : undefined}
            >
              <div className="flex items-center justify-between" style={{ padding: '14px 20px' }}>
                <div style={{ paddingRight: 16 }}>
                  <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-1)' }}>
                    {flag.label}
                  </h3>
                  <p className="text-xs" style={{ color: 'var(--color-text-3)', marginTop: 2 }}>
                    {flag.description}
                  </p>
                </div>
                <ToggleSwitch
                  testId={`experimental-${flag.key}`}
                  checked={on}
                  onChange={() => {
                    updateSection('features', { [flag.key]: !on });
                    save();
                  }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
