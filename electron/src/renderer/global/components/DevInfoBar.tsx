import React, { useRef, useState } from 'react';
import log from '#renderer/logger';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { DevInspector } from '#renderer/global/components/DevInspector';

interface DevInfo {
  mode: string;
  version: string;
  port: string;
  branch: string;
  worktree: string | null;
  slot: string | null;
}

// Cache survives component unmount/remount (list↔detail navigation in App.tsx
// causes a full DOM swap). Without this, the bar loads async on every remount,
// shifting the layout when it appears.
let _cachedInfo: DevInfo | null = null;

export function DevInfoBar(): React.ReactElement | null {
  const [info, setInfo] = useState<DevInfo | null>(_cachedInfo);
  const [inspecting, setInspecting] = useState(false);
  const [hoveredName, setHoveredName] = useState<string | null>(null);
  const inspectingRef = useRef(false);
  inspectingRef.current = inspecting;

  // Shift+A: toggle inspect on, or copy when already on (dev/sandbox only)
  const infoRef = useRef(info);
  infoRef.current = info;
  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!infoRef.current) return; // production — info never set
      const t = e.target as HTMLElement;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t.isContentEditable)
        return;
      if (e.key === 'A' && e.shiftKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        if (inspectingRef.current) {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const copy = (window as any).__devInspectorCopy;
          if (copy) copy();
        } else {
          setInspecting(true);
        }
      } else if (e.key === 'Escape' && inspectingRef.current) {
        setInspecting(false);
      }
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  });

  useMountEffect(() => {
    if (_cachedInfo) return;
    const load = async () => {
      if (!window.mandoAPI) return;
      const [mode, gatewayUrl, gitInfo] = await Promise.all([
        window.mandoAPI.appMode(),
        window.mandoAPI.gatewayUrl(),
        window.mandoAPI.devGitInfo(),
      ]);
      if (mode === 'production') return;
      if (!gatewayUrl) return;
      const port = new URL(gatewayUrl).port;
      const loaded: DevInfo = {
        mode: mode.toUpperCase(),
        version: __APP_VERSION__,
        port,
        branch: gitInfo.branch,
        worktree: gitInfo.worktree,
        slot: gitInfo.slot,
      };
      _cachedInfo = loaded;
      setInfo(loaded);
    };
    load().catch((err) => log.error('[DevInfoBar] failed to load:', err));
  });

  if (!info) return null;

  const prodLocalColor = 'var(--color-needs-human)';
  const previewColor = '#a855f7';
  const modeColor =
    info.mode === 'DEV'
      ? 'var(--color-accent)'
      : info.mode === 'PREVIEW'
        ? previewColor
        : info.mode === 'PROD-LOCAL'
          ? prodLocalColor
          : 'var(--color-text-2)';
  const tintColor =
    info.mode === 'DEV'
      ? 'var(--color-accent)'
      : info.mode === 'PREVIEW'
        ? previewColor
        : info.mode === 'PROD-LOCAL'
          ? prodLocalColor
          : 'var(--color-text-3)';
  const bg = `color-mix(in srgb, ${tintColor} 5%, transparent)`;
  const border = `color-mix(in srgb, ${tintColor} 20%, transparent)`;

  const btnStyle = {
    color: modeColor,
    background: 'transparent',
    border: 'none',
  };

  const btnClass =
    'cursor-pointer rounded px-1.5 py-0.5 font-mono text-[11px] opacity-60 transition-opacity hover:opacity-100';

  return (
    <>
      <DevInspector active={inspecting} onHover={setHoveredName} />
      <div
        data-dev-toolbar
        className="flex shrink-0 items-center gap-4 border-t px-4 py-1 font-mono text-[11px]"
        style={{ borderColor: border, background: bg, color: modeColor }}
      >
        <span className="font-semibold">{info.mode}</span>
        <span style={{ color: 'var(--color-text-3)' }}>v{info.version}</span>
        <span style={{ color: 'var(--color-text-3)' }}>
          <span style={{ color: 'var(--color-text-4)' }}>port:</span>
          {info.port}
        </span>
        {info.slot && (
          <span style={{ color: 'var(--color-text-3)' }}>
            <span style={{ color: 'var(--color-text-4)' }}>slot:</span>
            {info.slot}
          </span>
        )}
        <span style={{ color: 'var(--color-text-3)' }}>
          <span style={{ color: 'var(--color-text-4)' }}>branch:</span>
          {info.branch}
        </span>
        {info.worktree && (
          <span style={{ color: 'var(--color-text-3)' }}>
            <span style={{ color: 'var(--color-text-4)' }}>wt:</span>
            {info.worktree}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          {inspecting && hoveredName && (
            <span
              style={{
                color: 'color-mix(in srgb, var(--color-accent) 70%, transparent)',
                fontSize: 11,
              }}
              className="font-mono"
            >
              {hoveredName}
            </span>
          )}
          <button
            onClick={() => setInspecting((v) => !v)}
            className={btnClass}
            style={{
              ...btnStyle,
              opacity: inspecting ? 1 : undefined,
              color: inspecting ? 'var(--color-accent)' : modeColor,
            }}
          >
            {inspecting ? '● Inspect' : 'Inspect'}
          </button>
          <span style={{ color: 'var(--color-text-4)', fontSize: 11 }}>
            {inspecting ? '⇧A copy · Esc exit' : '⇧A'}
          </span>
          <button
            onClick={() => window.mandoAPI?.toggleDevTools()}
            className={btnClass}
            style={btnStyle}
          >
            DevTools
          </button>
        </div>
      </div>
    </>
  );
}
