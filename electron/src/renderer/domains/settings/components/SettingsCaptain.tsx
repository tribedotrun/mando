import React from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Card, CardContent } from '#renderer/components/ui/card';
import { useConfig } from '#renderer/hooks/queries';
import { useConfigSave } from '#renderer/hooks/mutations';
import { queryKeys } from '#renderer/queryKeys';
import type { MandoConfig, CaptainConfig } from '#renderer/types';
import { Switch } from '#renderer/components/ui/switch';

const EMPTY_CAPTAIN: CaptainConfig = {};
const CLAUDE_ARGS_DEFAULT = '--dangerously-skip-permissions';
const CODEX_ARGS_DEFAULT = '--full-auto';

export function SettingsCaptain(): React.ReactElement {
  const { data: config } = useConfig();
  const saveMut = useConfigSave();
  const qc = useQueryClient();
  const captain = config?.captain ?? EMPTY_CAPTAIN;

  const saveSection = (patch: Partial<CaptainConfig>) => {
    const current = qc.getQueryData<MandoConfig>(queryKeys.config.current()) ?? {};
    const updated: MandoConfig = {
      ...current,
      captain: { ...(current.captain || {}), ...patch },
    };
    saveMut.mutate(updated);
  };

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
