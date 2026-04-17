import React from 'react';
import { Card, CardContent } from '#renderer/global/ui/card';
import { useConfig, useConfigPatch } from '#renderer/domains/settings/runtime/hooks';
import {
  captainPatch,
  CLAUDE_ARGS_DEFAULT,
  CODEX_ARGS_DEFAULT,
} from '#renderer/global/service/configPatches';
import type { CaptainConfig } from '#renderer/global/types';
import { Switch } from '#renderer/global/ui/switch';

const EMPTY_CAPTAIN: CaptainConfig = {};

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
              <h3 className="text-sm font-medium text-muted-foreground">Default Terminal Agent</h3>
            </div>
            <select
              value={captain.defaultTerminalAgent ?? 'claude'}
              onChange={(e) => {
                saveSection({
                  defaultTerminalAgent: e.target.value as 'claude' | 'codex',
                });
              }}
              className="rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground"
            >
              <option value="claude">Claude Code</option>
              <option value="codex">Codex</option>
            </select>
          </div>
          <div>
            <h3 className="text-sm font-medium text-muted-foreground">Claude Code Args</h3>
            <input
              data-testid="captain-claude-terminal-args"
              type="text"
              value={captain.claudeTerminalArgs ?? CLAUDE_ARGS_DEFAULT}
              onChange={(e) => saveSection({ claudeTerminalArgs: e.target.value })}
              placeholder={CLAUDE_ARGS_DEFAULT}
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-1.5 font-mono text-sm text-foreground"
            />
          </div>
          <div>
            <h3 className="text-sm font-medium text-muted-foreground">Codex Args</h3>
            <input
              data-testid="captain-codex-terminal-args"
              type="text"
              value={captain.codexTerminalArgs ?? CODEX_ARGS_DEFAULT}
              onChange={(e) => saveSection({ codexTerminalArgs: e.target.value })}
              placeholder={CODEX_ARGS_DEFAULT}
              className="mt-1 w-full rounded-md border border-border bg-background px-3 py-1.5 font-mono text-sm text-foreground"
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
