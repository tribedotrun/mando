import React, { useRef, useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useDevInfo } from '#renderer/global/runtime/useAppInfo';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { DevInspector } from '#renderer/global/ui/DevInspector';
import { Badge } from '#renderer/global/ui/badge';
import { Button } from '#renderer/global/ui/button';

export function DevInfoBar(): React.ReactElement | null {
  const info = useDevInfo();
  const { openConfigFile, openDataDir, toggleDevTools } = useNativeActions();
  const [inspecting, setInspecting] = useState(false);
  const [hoveredName, setHoveredName] = useState<string | null>(null);
  const inspectingRef = useRef(false);
  inspectingRef.current = inspecting;

  // Shift+A: toggle inspect on, or copy when already on (dev/sandbox only)
  const infoRef = useRef(info);
  infoRef.current = info;
  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!infoRef.current) return;
      const t = e.target as HTMLElement;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t.isContentEditable)
        return;
      if (e.key === 'A' && e.shiftKey && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        if (inspectingRef.current) {
          const copy = window.__devInspectorCopy;
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

  if (!info) return null;

  const prodLocalColor = 'var(--needs-human)';
  const previewColor = '#a855f7';
  const modeColor =
    info.mode === 'DEV'
      ? 'var(--muted-foreground)'
      : info.mode === 'PREVIEW'
        ? previewColor
        : info.mode === 'PROD-LOCAL'
          ? prodLocalColor
          : 'var(--muted-foreground)';
  const tintColor =
    info.mode === 'DEV'
      ? 'var(--muted-foreground)'
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
        <span className="text-text-3">
          <span className="text-text-4">commit:</span>
          {info.commit}
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
              style={{ color: 'color-mix(in srgb, var(--muted-foreground) 70%, transparent)' }}
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
              color: inspecting ? 'var(--muted-foreground)' : modeColor,
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
            onClick={openConfigFile}
            className={btnClass}
            style={btnStyle}
          >
            Config
          </Button>
          <Button
            variant="ghost"
            size="xs"
            onClick={openDataDir}
            className={btnClass}
            style={btnStyle}
          >
            Data Dir
          </Button>
          <Button
            variant="ghost"
            size="xs"
            onClick={toggleDevTools}
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
