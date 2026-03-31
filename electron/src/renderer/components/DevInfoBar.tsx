import React, { useRef, useState } from 'react';
import log from '#renderer/logger';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { DevInspector } from '#renderer/components/DevInspector';

interface DevInfo {
  mode: string;
  version: string;
  port: string;
  branch: string;
  worktree: string | null;
  slot: string | null;
}

export function DevInfoBar(): React.ReactElement | null {
  const [info, setInfo] = useState<DevInfo | null>(null);
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
      setInfo({
        mode: mode.toUpperCase(),
        version: __APP_VERSION__,
        port,
        branch: gitInfo.branch,
        worktree: gitInfo.worktree,
        slot: gitInfo.slot,
      });
    };
    load().catch((err) => log.error('[DevInfoBar] failed to load:', err));
  });

  if (!info) return null;

  const modeColor = info.mode === 'DEV' ? 'var(--color-accent)' : 'var(--color-text-2)';
  const bg =
    info.mode === 'DEV'
      ? 'color-mix(in srgb, var(--color-accent) 5%, transparent)'
      : 'color-mix(in srgb, var(--color-text-3) 5%, transparent)';
  const border =
    info.mode === 'DEV'
      ? 'color-mix(in srgb, var(--color-accent) 20%, transparent)'
      : 'color-mix(in srgb, var(--color-text-3) 20%, transparent)';

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
            <span style={{ color: 'rgba(99, 102, 241, 0.7)', fontSize: 11 }} className="font-mono">
              {hoveredName}
            </span>
          )}
          <button
            onClick={() => setInspecting((v) => !v)}
            className={btnClass}
            style={{
              ...btnStyle,
              opacity: inspecting ? 1 : undefined,
              color: inspecting ? 'rgba(99, 102, 241, 1)' : modeColor,
            }}
          >
            {inspecting ? '● Inspect' : 'Inspect'}
          </button>
          <span style={{ color: 'var(--color-text-4)', fontSize: 10 }}>
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
