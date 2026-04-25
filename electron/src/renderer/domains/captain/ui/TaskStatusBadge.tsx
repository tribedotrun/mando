import React from 'react';
import type { SessionSummary, TaskItem } from '#renderer/global/types';
import {
  getStatusBadge,
  isStreamingStatus,
  resolvePausedBadge,
} from '#renderer/domains/captain/service/statusBadgeConfig';
import { fmtDuration, nowEpochSeconds } from '#renderer/global/service/utils';
import { StatusDot } from '#renderer/domains/captain/ui/CardFrame';

interface HeaderBadgeProps {
  item: TaskItem;
  sessions: SessionSummary[];
}

function Badge({
  color,
  pulse,
  children,
}: {
  color: string;
  pulse?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <span
      className="flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-0.5"
      style={{
        background: `color-mix(in srgb, ${color} 10%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 25%, transparent)`,
      }}
    >
      <StatusDot color={color} pulse={pulse} size="sm" />
      <span className="text-caption font-medium" style={{ color }}>
        {children}
      </span>
    </span>
  );
}

export function TaskStatusBadge({ item, sessions }: HeaderBadgeProps): React.ReactElement {
  // Paused takes precedence over the underlying lifecycle status: every
  // credential is in cooldown and captain skips dispatch until
  // `paused_until` passes. Service owns the time comparison.
  const paused = resolvePausedBadge(item.paused_until, nowEpochSeconds());
  if (paused) {
    return <Badge color={paused.color}>{paused.label}</Badge>;
  }
  const cfg = getStatusBadge(item.status);
  let label = cfg.label;
  if (isStreamingStatus(item.status)) {
    const active = sessions.find((ss) => ss.status === 'running');
    const dur = active ? (active.duration_ms ?? 0) / 1000 : 0;
    if (dur > 0) label = `${label} ${fmtDuration(dur)}`;
  }
  return (
    <Badge color={cfg.color} pulse={cfg.pulse}>
      {label}
    </Badge>
  );
}
