import React from 'react';

export const INPUT_CLS =
  'w-full rounded px-3 py-2 text-xs placeholder-[var(--color-text-3)] focus:outline-none';
export const INPUT_STYLE: React.CSSProperties = {
  border: '1px solid var(--color-border-subtle)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
  flex: 1,
};

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
                    borderRadius: 2,
                    background: i < step ? 'var(--color-accent)' : 'var(--color-border)',
                  }}
                />
              ))}
            </div>
            <span
              className="text-caption"
              style={{
                color: 'var(--color-text-3)',
                letterSpacing: '0.04em',
                textTransform: 'uppercase' as const,
              }}
            >
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
      <span style={{ color: ok ? 'var(--color-success)' : 'var(--color-danger)', fontSize: 13 }}>
        {ok ? '✓' : '✗'}
      </span>
      <span
        className="text-body"
        style={{ color: ok ? 'var(--color-text-1)' : 'var(--color-danger)' }}
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
    <button
      onClick={onClick}
      className="text-[13px]"
      style={{
        padding: '8px 16px',
        color: 'var(--color-text-2)',
        background: 'none',
        border: 'none',
        cursor: 'pointer',
      }}
    >
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
      className="text-[13px] font-medium disabled:opacity-50"
      style={{
        padding: '8px 16px',
        borderRadius: 'var(--radius-button)',
        border: '1px solid var(--color-border-subtle)',
        background: 'transparent',
        color: 'var(--color-text-2)',
        cursor: disabled ? 'default' : 'pointer',
        flexShrink: 0,
      }}
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
    <button
      onClick={onClick}
      disabled={disabled}
      className="text-[13px] font-semibold disabled:opacity-50 transition-colors hover:brightness-110 active:brightness-90"
      style={{
        padding: '8px 20px',
        borderRadius: 'var(--radius-button)',
        background: 'var(--color-accent)',
        color: 'var(--color-bg)',
        border: 'none',
        cursor: disabled ? 'default' : 'pointer',
      }}
    >
      {children}
    </button>
  );
}
