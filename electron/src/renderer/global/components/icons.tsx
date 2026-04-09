import React from 'react';

/* ── Progress circle icons (task status) ── */

const S = 16;

/** Dotted circle -- queued / new (not started) */
export function IconQueued() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle
        cx="8"
        cy="8"
        r="6"
        stroke="var(--text-3)"
        strokeWidth="1.5"
        strokeDasharray="2.5 2.5"
      />
    </svg>
  );
}

/** Half-filled circle -- in progress / clarifying */
export function IconWorking() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--muted-foreground)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12V2z" fill="var(--muted-foreground)" />
    </svg>
  );
}

/** Three-quarter circle -- captain reviewing (almost done) */
export function IconReviewing() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--muted-foreground)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12A6 6 0 0 1 2 8h6V2z" fill="var(--muted-foreground)" />
    </svg>
  );
}

/** Half circle orange -- rework */
export function IconRework() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--stale)" strokeWidth="1.5" />
      <path d="M8 2a6 6 0 0 1 0 12V2z" fill="var(--stale)" />
    </svg>
  );
}

/** Open circle -- handed off (parked) */
export function IconHandedOff() {
  return (
    <svg width={S} height={S} viewBox="0 0 16 16" fill="none">
      <circle cx="8" cy="8" r="6" stroke="var(--text-3)" strokeWidth="1.5" />
    </svg>
  );
}

/* ── PR state icons (GitHub Octicons) ── */

export type PrState = 'open' | 'merged' | 'closed';

const MERGE_PATH =
  'M5.45 5.154A4.25 4.25 0 0 0 9.25 7.5h1.378a2.251 2.251 0 1 1 0 1.5H9.25A5.734 5.734 0 0 1 5 7.123v3.505a2.25 2.25 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.95-.218ZM4.25 13.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Zm8.5-4.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5ZM5 3.25a.75.75 0 1 0 0 .005V3.25Z';

export function PrIcon({ state }: { state: PrState }): React.ReactElement {
  const color =
    state === 'open' ? 'var(--foreground)' : state === 'merged' ? 'var(--text-3)' : 'var(--text-4)';

  if (state === 'merged') {
    return (
      <svg width="14" height="14" viewBox="0 0 16 16" fill={color} className="shrink-0">
        <path d={MERGE_PATH} />
      </svg>
    );
  }

  if (state === 'closed') {
    return (
      <svg width="14" height="14" viewBox="0 0 16 16" fill={color} className="shrink-0">
        <path d="M3.25 1A2.25 2.25 0 0 1 4 5.372v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.251 2.251 0 0 1 3.25 1Zm9.5 5.5a.75.75 0 0 1 .75.75v3.378a2.251 2.251 0 1 1-1.5 0V7.25a.75.75 0 0 1 .75-.75Zm-2.03-5.273a.75.75 0 0 1 1.06 0l.97.97.97-.97a.748.748 0 0 1 1.265.332.75.75 0 0 1-.205.729l-.97.97.97.97a.751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018l-.97-.97-.97.97a.749.749 0 0 1-1.275-.326.749.749 0 0 1 .215-.734l.97-.97-.97-.97a.75.75 0 0 1 0-1.06ZM2.5 3.25a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0ZM3.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm9.5 0a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Z" />
      </svg>
    );
  }

  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill={color} className="shrink-0">
      <path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354ZM3.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm0 9.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm8.25.75a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z" />
    </svg>
  );
}

/** Standalone merge icon for buttons */
export function MergeIcon(): React.ReactElement {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
      <path d={MERGE_PATH} />
    </svg>
  );
}

/* ── App brand icons (extracted from macOS app bundles) ── */

const FINDER_PNG =
  'data:image/png;base64,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';

const CURSOR_PNG =
  'data:image/png;base64,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';

export function FinderIcon({ size = 16 }: { size?: number }): React.ReactElement {
  return <img src={FINDER_PNG} width={size} height={size} alt="Finder" className="shrink-0" />;
}

export function CursorIcon({ size = 16 }: { size?: number }): React.ReactElement {
  return <img src={CURSOR_PNG} width={size} height={size} alt="Cursor" className="shrink-0" />;
}
