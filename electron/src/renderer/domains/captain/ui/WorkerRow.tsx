import React, { useState } from 'react';
import { MoreVertical } from 'lucide-react';
import { getWorkerPhase, PHASE_COLORS } from '#renderer/domains/captain/service/metricsHelpers';
import { fmtRuntime, shortRepo } from '#renderer/global/service/utils';
import type { WorkerDetail } from '#renderer/global/types';
import { StatusDot } from '#renderer/domains/captain/ui/CardFrame';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '#renderer/global/ui/primitives/dropdown-menu';
import { Button } from '#renderer/global/ui/primitives/button';

export function WorkerRow({
  worker,
  stale,
  onNudge,
  onStop,
}: {
  worker: WorkerDetail;
  stale: boolean;
  onNudge?: (worker: WorkerDetail) => void;
  onStop?: (worker: WorkerDetail) => void | Promise<void>;
}): React.ReactElement {
  const [menuOpen, setMenuOpen] = useState(false);
  const [stopping, setStopping] = useState(false);
  const hasActions = !!onNudge || !!onStop;
  const phase = getWorkerPhase(worker, stale);
  const colors = PHASE_COLORS[phase];

  return (
    <div className="group relative flex min-h-[26px] items-center gap-2.5 rounded px-4 py-1 transition-colors duration-100 hover:bg-white/[0.03]">
      <StatusDot color={colors.dot} size="sm" />

      <span
        className="min-w-0 flex-1 truncate text-[12px] leading-4"
        style={{ color: colors.text }}
      >
        {worker.title}
      </span>

      <span className="max-w-[80px] shrink-0 truncate text-[11px] leading-[14px] text-text-4">
        {shortRepo(worker.project)}
      </span>

      <span
        className="w-12 shrink-0 text-right text-[11px] leading-[14px]"
        style={{ color: colors.duration }}
      >
        {fmtRuntime(
          phase === 'reviewing' || phase === 'merging'
            ? (worker.last_activity_at ?? undefined)
            : (worker.started_at ?? undefined),
        )}
      </span>

      {colors.label && (
        <span className="shrink-0 text-[11px] leading-[14px]" style={{ color: colors.dot }}>
          {colors.label}
        </span>
      )}

      {hasActions && (
        <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="Worker actions"
              className={`shrink-0 opacity-0 transition-opacity group-hover:opacity-100 ${menuOpen ? 'opacity-100' : ''}`}
            >
              <MoreVertical size={10} />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {onNudge && <DropdownMenuItem onSelect={() => onNudge(worker)}>Nudge</DropdownMenuItem>}
            {onStop && (
              <DropdownMenuItem
                variant="destructive"
                disabled={stopping}
                onSelect={(event) => {
                  event.preventDefault();
                  setStopping(true);
                  const result = onStop(worker);
                  if (result instanceof Promise) {
                    void result.finally(() => {
                      setStopping(false);
                      setMenuOpen(false);
                    });
                  } else {
                    setStopping(false);
                    setMenuOpen(false);
                  }
                }}
              >
                {stopping ? 'Stopping...' : 'Stop'}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
