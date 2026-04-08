import React from 'react';
import { Card, CardContent } from '#renderer/components/ui/card';
import { Separator } from '#renderer/components/ui/separator';
import {
  useSettingsStore,
  type FeaturesConfig,
} from '#renderer/domains/settings/stores/settingsStore';
import { Switch } from '#renderer/components/ui/switch';

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
      <h2 className="text-lg font-semibold text-foreground">Experimental</h2>
      <p className="mb-6 mt-1 text-sm text-muted-foreground">
        Alpha features. These may change or be removed at any time.
      </p>

      <Card className="py-0">
        {FLAGS.map((flag, i) => {
          const on = !!features[flag.key];
          return (
            <React.Fragment key={flag.key}>
              {i > 0 && <Separator />}
              <CardContent className="py-3.5">
                <div className="flex items-center justify-between">
                  <div className="pr-4">
                    <h3 className="text-sm font-medium text-foreground">{flag.label}</h3>
                    <p className="mt-0.5 text-xs text-muted-foreground">{flag.description}</p>
                  </div>
                  <Switch
                    data-testid={`experimental-${flag.key}`}
                    checked={on}
                    onCheckedChange={(checked) => {
                      updateSection('features', { [flag.key]: checked });
                      void save();
                    }}
                  />
                </div>
              </CardContent>
            </React.Fragment>
          );
        })}
      </Card>
    </div>
  );
}
