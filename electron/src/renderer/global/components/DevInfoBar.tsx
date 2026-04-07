import React, { useRef, useState } from 'react';
import log from '#renderer/logger';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { DevInspector } from '#renderer/global/components/DevInspector';
import { Badge } from '#renderer/components/ui/badge';
import { Button } from '#renderer/components/ui/button';

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

  const prodLocalColor = 'var(--needs-human)';
  const previewColor = '#a855f7';
  const modeColor =
    info.mode === 'DEV'
      ? 'var(--primary)'
      : info.mode === 'PREVIEW'
        ? previewColor
        : info.mode === 'PROD-LOCAL'
          ? prodLocalColor
          : 'var(--muted-foreground)';
  const tintColor =
    info.mode === 'DEV'
      ? 'var(--primary)'
      : info.mode === 'PREVIEW'
        ? previewColor
        : info.mode === 'PROD-LOCAL'
          ? prodLocalColor
          : 'var(--text-3)';
  const bg = `color-mix(in srgb, ${tintColor} 5%, transparent)`;

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
        className="flex shrink-0 items-center gap-4 px-4 py-1 font-mono text-[11px]"
        style={{ background: bg, color: modeColor }}
      >
        <Badge
          variant="outline"
          className="px-1.5 py-0 text-[11px] font-semibold"
          style={{ color: modeColor }}
        >
          {info.mode}
        </Badge>
        <span className="text-text-3">v{info.version}</span>
        <span className="text-text-3">
          <span className="text-text-4">port:</span>
          {info.port}
        </span>
        {info.slot && (
          <span className="text-text-3">
            <span className="text-text-4">slot:</span>
            {info.slot}
          </span>
        )}
        <span className="text-text-3">
          <span className="text-text-4">branch:</span>
          {info.branch}
        </span>
        {info.worktree && (
          <span className="text-text-3">
            <span className="text-text-4">wt:</span>
            {info.worktree}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          {inspecting && hoveredName && (
            <span
              style={{ color: 'color-mix(in srgb, var(--primary) 70%, transparent)' }}
              className="font-mono text-[11px]"
            >
              {hoveredName}
            </span>
          )}
          <Button
            variant="ghost"
            size="xs"
            onClick={() => setInspecting((v) => !v)}
            className={btnClass}
            style={{
              ...btnStyle,
              opacity: inspecting ? 1 : undefined,
              color: inspecting ? 'var(--primary)' : modeColor,
            }}
          >
            {inspecting ? '● Inspect' : 'Inspect'}
          </Button>
          <span className="text-[11px] text-text-4">
            {inspecting ? '⇧A copy · Esc exit' : '⇧A'}
          </span>
          <Button
            variant="ghost"
            size="xs"
            onClick={() => window.mandoAPI?.toggleDevTools()}
            className={btnClass}
            style={btnStyle}
          >
            DevTools
          </Button>
        </div>
      </div>
    </>
  );
}
