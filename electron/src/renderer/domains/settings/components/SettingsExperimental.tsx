import React from 'react';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import type { FeaturesConfig } from '#renderer/domains/settings/stores/settingsStore';
import { Switch } from '#renderer/global/components/Switch';

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
      <h2 className="text-lg font-semibold text-text-1">Experimental</h2>
      <p className="mt-1 text-sm text-text-3" style={{ marginBottom: 24 }}>
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
                  <h3 className="text-sm font-medium text-text-1">{flag.label}</h3>
                  <p className="text-xs text-text-3" style={{ marginTop: 2 }}>
                    {flag.description}
                  </p>
                </div>
                <Switch
                  testId={`experimental-${flag.key}`}
                  checked={on}
                  onCheckedChange={(checked) => {
                    updateSection('features', { [flag.key]: checked });
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
