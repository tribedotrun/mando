import React from 'react';

export const INPUT_CLS =
  'w-full rounded px-2.5 py-1.5 text-xs placeholder-[var(--color-text-3)] focus:outline-none';
export const INPUT_STYLE: React.CSSProperties = {
  border: '1px solid var(--color-border-subtle)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
  flex: 1,
};

export function CenteredCard({
  children,
  ...rest
}: React.HTMLAttributes<HTMLDivElement>): React.ReactElement {
  return (
    <div
      className="flex h-full flex-col justify-start"
      style={{ background: 'var(--color-bg)', paddingTop: '30vh' }}
      {...rest}
    >
      <div style={{ width: '100%', maxWidth: 560, padding: 32, margin: '0 auto' }}>{children}</div>
    </div>
  );
}

export function StepDots({
  total,
  current,
}: {
  total: number;
  current: number;
}): React.ReactElement {
  return (
    <div className="flex items-center justify-center" style={{ gap: 10, marginBottom: 24 }}>
      {Array.from({ length: total }, (_, i) => (
        <span
          key={i}
          style={{
            width: i === current ? 8 : 7,
            height: i === current ? 8 : 7,
            borderRadius: '50%',
            background: i === current ? 'var(--color-accent)' : 'var(--color-border)',
            transition: 'all 0.2s',
          }}
        />
      ))}
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
      className="text-[13px] font-semibold disabled:opacity-50"
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
