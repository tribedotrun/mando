import React from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Card, CardContent } from '#renderer/components/ui/card';
import { Separator } from '#renderer/components/ui/separator';
import { useConfig } from '#renderer/hooks/queries';
import { useConfigSave } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import type { MandoConfig, FeaturesConfig } from '#renderer/types';
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
  const { data: config } = useConfig();
  const saveMut = useConfigSave();
  const qc = useQueryClient();
  const features = config?.features ?? EMPTY_FEATURES;

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
                      const current =
                        qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? {};
                      const updated: MandoConfig = {
                        ...current,
                        features: { ...(current.features || {}), [flag.key]: checked },
                      };
                      saveMut.mutate(updated);
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
