import React from 'react';

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
    <div className="relative flex h-full" style={{ background: 'var(--color-bg)' }} {...rest}>
      <div
        className="absolute inset-x-0 top-0 z-10 h-8"
        style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
      />
      <div
        className="flex flex-col justify-center"
        style={{
          width: 300,
          padding: '0 40px',
          paddingBottom: 80,
          background: 'var(--color-surface-1)',
          borderRight: '1px solid var(--color-border-subtle)',
          flexShrink: 0,
        }}
      >
        {step != null && total != null && (
          <div style={{ marginBottom: 24 }}>
            <div style={{ display: 'flex', gap: 6, marginBottom: 10 }}>
              {Array.from({ length: total }, (_, i) => (
                <div
                  key={i}
                  style={{
                    width: 36,
                    height: 3,
                    borderRadius: 4,
                    background: i < step ? 'var(--color-accent)' : 'var(--color-border)',
                  }}
                />
              ))}
            </div>
            <span className="text-label" style={{ color: 'var(--color-text-3)' }}>
              Step {step} of {total}
            </span>
          </div>
        )}
        <h2 className="text-heading" style={{ color: 'var(--color-text-1)', marginBottom: 8 }}>
          {title}
        </h2>
        {subtitle && (
          <p className="text-body" style={{ color: 'var(--color-text-3)', lineHeight: 1.6 }}>
            {subtitle}
          </p>
        )}
      </div>
      <div className="flex flex-1 items-center" style={{ padding: '0 64px', paddingBottom: 80 }}>
        <div style={{ width: '100%', maxWidth: 480 }}>{children}</div>
      </div>
    </div>
  );
}

export function CheckRow({ ok, label }: { ok: boolean; label: string }): React.ReactElement {
  return (
    <div className="flex items-center" style={{ gap: 8 }}>
      <span style={{ color: ok ? 'var(--color-success)' : 'var(--color-error)', fontSize: 13 }}>
        {ok ? '✓' : '✗'}
      </span>
      <span
        className="text-body"
        style={{ color: ok ? 'var(--color-text-1)' : 'var(--color-error)' }}
      >
        {label}
      </span>
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
    <button onClick={onClick} className="btn btn-text">
      {children}
    </button>
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
    <button
      onClick={onClick}
      disabled={disabled}
      className="btn btn-ghost"
      style={{ flexShrink: 0 }}
    >
      {children}
    </button>
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
    <button onClick={onClick} disabled={disabled} className="btn btn-primary">
      {children}
    </button>
  );
}
