import React from 'react';
import { Card, CardContent } from '#renderer/components/ui/card';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import type { CaptainConfig } from '#renderer/domains/settings/stores/settingsStore';
import { Switch } from '#renderer/components/ui/switch';

const EMPTY_CAPTAIN: CaptainConfig = {};

export function SettingsCaptain(): React.ReactElement {
  const captain = useSettingsStore((s) => s.config.captain ?? EMPTY_CAPTAIN);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);

  return (
    <div data-testid="settings-captain">
      <h2 className="text-heading text-foreground">Captain</h2>
      <p className="mb-6 mt-1 text-caption text-muted-foreground">
        Ticks every 30 seconds to check task progress, review PRs, and intervene when needed.
      </p>

      <Card className="py-4">
        <CardContent>
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-muted-foreground">Auto Tick</h3>
            </div>
            <Switch
              data-testid="captain-auto-tick"
              checked={!!captain.autoSchedule}
              onCheckedChange={(checked) => {
                updateSection('captain', { autoSchedule: checked });
                save();
              }}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
