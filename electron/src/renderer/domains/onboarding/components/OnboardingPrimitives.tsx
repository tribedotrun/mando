import React from 'react';
import { Button } from '#renderer/components/ui/button';

export function SetupLayout({
  step,
  total,
  title,
  subtitle,
  children,
  ...rest
}: {
  step?: number;
  total?: number;
  title: string;
  subtitle?: string;
  children: React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, 'title'>): React.ReactElement {
  return (
    <div className="relative flex h-full bg-background" {...rest}>
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
      <div className="flex w-[300px] shrink-0 flex-col justify-center bg-card px-10 pb-20">
        {step != null && total != null && (
          <div className="mb-6">
            <div className="mb-2.5 flex gap-1.5">
              {Array.from({ length: total }, (_, i) => (
                <div
                  key={i}
                  className={`h-[3px] w-9 rounded ${i < step ? 'bg-primary' : 'bg-border'}`}
                />
              ))}
            </div>
            <span className="text-label text-muted-foreground">
              Step {step} of {total}
            </span>
          </div>
        )}
        <h2 className="mb-2 text-heading text-foreground">{title}</h2>
        {subtitle && <p className="text-body leading-relaxed text-muted-foreground">{subtitle}</p>}
      </div>
      <div className="flex flex-1 items-center px-16 pb-20">
        <div className="w-full max-w-[480px]">{children}</div>
      </div>
    </div>
  );
}

export function CheckRow({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <div className="flex items-center gap-2">
      <span className={`text-[13px] ${ok ? 'text-success' : 'text-destructive'}`}>
        {ok ? '\u2713' : '\u2717'}
      </span>
      <span className={`text-body ${ok ? 'text-foreground' : 'text-destructive'}`}>{label}</span>
    </div>
  );
}

export function GhostButton({
  onClick,
  children,
}: {
  onClick: () => void;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Button variant="ghost" onClick={onClick}>
      {children}
    </Button>
  );
}

export function OutlineButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Button variant="outline" onClick={onClick} disabled={disabled} className="shrink-0">
      {children}
    </Button>
  );
}

export function PrimaryButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <Button onClick={onClick} disabled={disabled}>
      {children}
    </Button>
  );
}
