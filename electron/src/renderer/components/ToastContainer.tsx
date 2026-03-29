import React, { useCallback } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useToastStore } from '#renderer/stores/toastStore';
import type { Toast } from '#renderer/stores/toastStore';

type ToastVariant = 'success' | 'error' | 'info';

/* ── Accent colors per variant ── */

const ACCENT_COLOR: Record<ToastVariant, string> = {
  success: 'var(--color-success)',
  error: 'var(--color-error)',
  info: 'var(--color-accent)',
};

/* ── Icons ── */

function CheckIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.5" />
      <path
        d="M5 8L7 10L11 6"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ErrorIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.5" />
      <path
        d="M6 6L10 10M10 6L6 10"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

function InfoIcon(): React.ReactElement {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="7" stroke="currentColor" strokeWidth="1.5" />
      <path d="M8 7V11" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      <circle cx="8" cy="5" r="0.75" fill="currentColor" />
    </svg>
  );
}

const VARIANT_ICON: Record<ToastVariant, () => React.ReactElement> = {
  success: CheckIcon,
  error: ErrorIcon,
  info: InfoIcon,
};

/* ── Single toast item ── */

function ToastItem({
  toast,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: () => void;
}): React.ReactElement {
  useMountEffect(() => {
    const timer = setTimeout(onDismiss, 4000);
    return () => clearTimeout(timer);
  });

  const accent = ACCENT_COLOR[toast.variant];
  const Icon = VARIANT_ICON[toast.variant];

  return (
    <div
      className="flex items-start gap-3 p-3"
      style={{
        minWidth: 280,
        maxWidth: 360,
        background: 'var(--color-surface-2)',
        border: '1px solid var(--color-border-subtle)',
        borderRadius: 8,
        animation: 'toast-in 200ms ease-out',
      }}
    >
      <span style={{ color: accent, flexShrink: 0, marginTop: 2 }}>
        <Icon />
      </span>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="text-body font-medium" style={{ color: 'var(--color-text-1)' }}>
          {toast.message}
        </span>
        {toast.detail && (
          <span className="text-caption" style={{ color: 'var(--color-text-2)', marginTop: 2 }}>
            {toast.detail}
          </span>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2" style={{ marginTop: 2 }}>
        {toast.onUndo && (
          <button
            onClick={() => {
              toast.onUndo?.();
              onDismiss();
            }}
            className="text-caption font-medium"
            style={{
              color: accent,
              cursor: 'pointer',
              background: 'none',
              border: 'none',
              padding: 0,
            }}
          >
            Undo
          </button>
        )}
        <button
          onClick={onDismiss}
          className="flex items-center justify-center"
          style={{
            color: 'var(--color-text-3)',
            cursor: 'pointer',
            background: 'none',
            border: 'none',
            padding: 0,
          }}
          aria-label="Dismiss"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path
              d="M4 4L10 10M10 4L4 10"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}

/* ── Container ── */

export function ToastContainer(): React.ReactElement {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  const stableDismiss = useCallback((id: string) => dismiss(id), [dismiss]);

  return (
    <>
      <style>{`
        @keyframes toast-in {
          from { opacity: 0; transform: translateY(8px); }
          to   { opacity: 1; transform: translateY(0); }
        }
      `}</style>
      <div
        className="fixed z-[400] flex flex-col-reverse gap-2"
        style={{ bottom: 16, right: 16, pointerEvents: 'none' }}
      >
        {toasts.map((toast) => (
          <div key={toast.id} style={{ pointerEvents: 'auto' }}>
            <ToastItem toast={toast} onDismiss={() => stableDismiss(toast.id)} />
          </div>
        ))}
      </div>
    </>
  );
}
