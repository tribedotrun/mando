import React from 'react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { useConfig, useConfigPatch } from '#renderer/domains/settings/runtime/hooks';
import { captainPatch } from '#renderer/global/service/configPatches';
import type { CaptainConfig } from '#renderer/global/types';
import { Switch } from '#renderer/global/ui/primitives/switch';

const EMPTY_CAPTAIN: CaptainConfig = Object.freeze({});

export function SettingsCaptain(): React.ReactElement {
  const { data: config } = useConfig();
  const { save } = useConfigPatch();
  const captain = config?.captain ?? EMPTY_CAPTAIN;

  const saveSection = (patch: Partial<CaptainConfig>) => save(captainPatch(patch));

  return (
    <div data-testid="settings-captain">
      <h2 className="text-heading text-foreground">Captain</h2>
      <p className="mb-6 mt-1 text-caption text-muted-foreground">
        Ticks every 30 seconds to check task progress, review PRs, and intervene when needed.
      </p>

      <Card className="py-4">
        <CardContent className="space-y-5">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-muted-foreground">Auto Tick</h3>
            </div>
            <Switch
              data-testid="captain-auto-tick"
              checked={!!captain.autoSchedule}
              onCheckedChange={(checked) => {
                saveSection({ autoSchedule: checked });
              }}
            />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-muted-foreground">
                Auto-merge high-confidence tasks
              </h3>
            </div>
            <Switch
              data-testid="captain-auto-merge"
              checked={!!captain.autoMerge}
              onCheckedChange={(checked) => {
                saveSection({ autoMerge: checked });
              }}
            />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-muted-foreground">Max Concurrent Workers</h3>
            </div>
            <select
              data-testid="captain-max-concurrent-workers"
              value={captain.maxConcurrentWorkers ?? 3}
              onChange={(e) => {
                saveSection({ maxConcurrentWorkers: +e.target.value });
              }}
              className="rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground"
            >
              {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-muted-foreground">Default Task Agent</h3>
            </div>
            <select
              value={captain.defaultTaskAgent ?? 'codex'}
              onChange={(e) => {
                saveSection({
                  defaultTaskAgent: e.target.value as 'claude' | 'codex',
                });
              }}
              className="rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground"
            >
              <option value="codex">Codex</option>
              <option value="claude">Claude Code</option>
            </select>
          </div>
          <div className="flex items-center justify-between gap-6">
            <div>
              <h3 className="text-sm font-medium text-muted-foreground">GLM worker by default</h3>
              <p className="mt-0.5 text-caption text-muted-foreground">
                New tasks build with Z.ai GLM 5.2 unless turned off per task.
              </p>
            </div>
            <Switch
              data-testid="captain-default-glm"
              checked={!!captain.defaultGlmImplementation}
              onCheckedChange={(checked) => {
                saveSection({ defaultGlmImplementation: checked });
              }}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
