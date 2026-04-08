import React from 'react';
import { cn } from '#renderer/cn';

interface CardShellProps {
  color: string;
  children: React.ReactNode;
  className?: string;
}

export function CardShell({ color, children, className }: CardShellProps): React.ReactElement {
  return (
    <div
      className={cn('flex items-center gap-2 rounded-lg px-4 py-3', className)}
      style={{
        background: `color-mix(in srgb, ${color} 6%, transparent)`,
        border: `1px solid color-mix(in srgb, ${color} 20%, transparent)`,
      }}
    >
      {children}
    </div>
  );
}

interface StatusDotProps {
  color: string;
  pulse?: boolean;
  size?: 'sm' | 'default';
}

export function StatusDot({ color, pulse, size = 'default' }: StatusDotProps): React.ReactElement {
  const sizeClass = size === 'sm' ? 'h-1 w-1' : 'h-2 w-2';
  return (
    <span
      aria-hidden="true"
      className={cn('inline-block shrink-0 rounded-full', sizeClass, pulse && 'animate-pulse')}
      style={{ background: color }}
    />
  );
}

export function Sep(): React.ReactElement {
  return <span className="text-caption text-text-4">&middot;</span>;
}
