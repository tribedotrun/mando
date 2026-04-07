import React, { useRef, useState } from 'react';
import { copyToClipboard, round } from '#renderer/utils';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

interface FiberNode {
  type: string | { name?: string; displayName?: string } | null;
  _debugSource?: { fileName: string; lineNumber: number; columnNumber: number };
  return: FiberNode | null;
  memoizedProps?: Record<string, unknown>;
}

interface InspectResult {
  component: string;
  file: string | null;
  line: number | null;
  element: string;
  nearbyText: string | null;
  parents: string[];
  props: string[];
  context: Record<string, string>;
}

function getFiber(el: HTMLElement): FiberNode | null {
  const key = Object.keys(el).find(
    (k) => k.startsWith('__reactFiber$') || k.startsWith('__reactInternalInstance$'),
  );
  return key ? (el as never)[key] : null;
}

function getComponentName(fiber: FiberNode): string | null {
  if (!fiber.type) return null;
  if (typeof fiber.type === 'string') return null;
  return fiber.type.displayName || fiber.type.name || null;
}

function findOwnerComponent(fiber: FiberNode): {
  name: string;
  source: FiberNode['_debugSource'] | null;
  props: string[];
  fiber: FiberNode;
} | null {
  let current: FiberNode | null = fiber;
  while (current) {
    const name = getComponentName(current);
    if (name) {
      const propKeys = current.memoizedProps
        ? Object.keys(current.memoizedProps).filter((k) => k !== 'children')
        : [];
      return { name, source: current._debugSource ?? null, props: propKeys, fiber: current };
    }
    current = current.return;
  }
  return null;
}

function getParentChain(fiber: FiberNode, limit = 5): string[] {
  const chain: string[] = [];
  let current = fiber.return;
  while (current && chain.length < limit) {
    const name = getComponentName(current);
    if (name) chain.push(name);
    current = current.return;
  }
  return chain;
}

function truncateElement(el: HTMLElement): string {
  const tag = el.tagName.toLowerCase();
  const cls = el.className ? ` class="${String(el.className).slice(0, 60)}"` : '';
  const text = el.textContent?.slice(0, 50) ?? '';
  return `<${tag}${cls}>${text}</${tag}>`;
}

function findNearbyText(el: HTMLElement): string | null {
  let current: HTMLElement | null = el;
  for (let i = 0; i < 4; i++) {
    current = current.parentElement;
    if (!current) break;
    const text = current.textContent?.trim() ?? '';
    if (text.length > 2 && text.length < 200) return text.slice(0, 100);
  }
  return null;
}

const SKIP_PROPS = new Set([
  'children',
  'className',
  'style',
  'key',
  'ref',
  'onClick',
  'onChange',
  'onSubmit',
  'onClose',
  'onBack',
  'onSelect',
  'onFilter',
  'onTabChange',
  'onProjectFilter',
  'onNewTask',
  'onOpenSettings',
  'onAddProject',
  'data-testid',
]);

/** Write `val` into `ctx[key]` if it is a primitive string/number worth surfacing. */
function collectPrimitive(ctx: Record<string, string>, key: string, val: unknown): void {
  if (SKIP_PROPS.has(key) || key in ctx || key.startsWith('on')) return;
  if (typeof val === 'string' && val.length > 0 && val.length < 200) {
    ctx[key] = val;
  } else if (typeof val === 'number') {
    ctx[key] = String(val);
  }
}

function extractContext(fiber: FiberNode): Record<string, string> {
  const ctx: Record<string, string> = {};
  let current: FiberNode | null = fiber;
  let depth = 0;
  while (current && depth < 8 && Object.keys(ctx).length < 10) {
    if (getComponentName(current) && current.memoizedProps) {
      for (const [key, val] of Object.entries(current.memoizedProps)) {
        if (typeof val === 'object' && val && !Array.isArray(val)) {
          for (const [nk, nv] of Object.entries(val as Record<string, unknown>)) {
            collectPrimitive(ctx, nk, nv);
          }
        } else {
          collectPrimitive(ctx, key, val);
        }
      }
    }
    current = current.return;
    depth++;
  }
  return ctx;
}

function buildInspectResult(el: HTMLElement): InspectResult | null {
  const fiber = getFiber(el);
  if (!fiber) return null;
  const owner = findOwnerComponent(fiber);
  if (!owner) return null;
  return {
    component: owner.name,
    file: owner.source?.fileName?.replace(/^.*\/electron\/src\//, 'src/') ?? null,
    line: owner.source?.lineNumber ?? null,
    element: truncateElement(el),
    nearbyText: findNearbyText(el),
    parents: getParentChain(owner.fiber),
    props: owner.props,
    context: extractContext(owner.fiber),
  };
}

function buildComponentMap(): Array<{
  component: string;
  file: string | null;
  line: number | null;
  rect: { x: number; y: number; w: number; h: number };
  props: string[];
}> {
  const seen = new Set<string>();
  const results: Array<{
    component: string;
    file: string | null;
    line: number | null;
    rect: { x: number; y: number; w: number; h: number };
    props: string[];
  }> = [];

  function walk(el: Element) {
    if (!(el instanceof HTMLElement)) return;
    const fiber = getFiber(el);
    if (fiber) {
      const owner = findOwnerComponent(fiber);
      if (owner) {
        const rect = el.getBoundingClientRect();
        const key = `${owner.name}:${round(rect.x)}:${round(rect.y)}`;
        if (!seen.has(key) && rect.width > 0 && rect.height > 0) {
          seen.add(key);
          results.push({
            component: owner.name,
            file: owner.source?.fileName?.replace(/^.*\/electron\/src\//, 'src/') ?? null,
            line: owner.source?.lineNumber ?? null,
            rect: {
              x: round(rect.x),
              y: round(rect.y),
              w: round(rect.width),
              h: round(rect.height),
            },
            props: owner.props,
          });
        }
      }
    }
    for (const child of el.children) walk(child);
  }

  walk(document.body);
  return results;
}

// Expose for agent workflow — called via page.evaluate() from electron-ctl
function installGlobals(doCopyRef: React.RefObject<() => void>) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (window as any).__buildComponentMap = buildComponentMap;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (window as any).__devInspectorCopy = () => doCopyRef.current();
}

function removeGlobals() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  delete (window as any).__buildComponentMap;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  delete (window as any).__devInspectorCopy;
}

export function DevInspector({
  active,
  onHover,
}: {
  active: boolean;
  onHover: (name: string | null) => void;
}): React.ReactElement | null {
  const highlightRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const hoveredRef = useRef<HTMLElement | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const activeRef = useRef(active);
  activeRef.current = active;
  const onHoverRef = useRef(onHover);
  onHoverRef.current = onHover;

  useMountEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!activeRef.current || !highlightRef.current || !labelRef.current) return;
      const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
      // When cursor enters toolbar, hide highlight but keep hoveredRef so Copy works
      if (!el || el.closest('[data-dev-toolbar]')) {
        highlightRef.current.style.display = 'none';
        labelRef.current.style.display = 'none';
        return;
      }

      hoveredRef.current = el;
      const rect = el.getBoundingClientRect();
      highlightRef.current.style.display = 'block';
      highlightRef.current.style.top = `${rect.top}px`;
      highlightRef.current.style.left = `${rect.left}px`;
      highlightRef.current.style.width = `${rect.width}px`;
      highlightRef.current.style.height = `${rect.height}px`;

      const fiber = getFiber(el);
      if (fiber) {
        const owner = findOwnerComponent(fiber);
        if (owner) {
          labelRef.current.style.display = 'block';
          labelRef.current.style.top = `${rect.top - 22 < 0 ? 0 : rect.top - 22}px`;
          labelRef.current.style.left = `${rect.left}px`;
          labelRef.current.textContent = owner.name;
          onHoverRef.current(owner.name);
          return;
        }
      }
      labelRef.current.style.display = 'none';
      onHoverRef.current(null);
    };

    document.addEventListener('mousemove', onMouseMove, true);
    return () => {
      document.removeEventListener('mousemove', onMouseMove, true);
    };
  });

  // Called by DevInfoBar's Copy button
  const doCopy = async () => {
    const el = hoveredRef.current;
    if (!el) return;
    const info = buildInspectResult(el);
    if (!info) return;
    const ok = await copyToClipboard(JSON.stringify(info));
    if (ok) {
      setToast(`${info.component}${info.context.title ? ' — ' + info.context.title : ''}`);
      setTimeout(() => setToast(null), 2000);
    }
  };

  const doCopyRef = useRef(doCopy);
  doCopyRef.current = doCopy;

  // Attach globals on mount, clean up on unmount
  useMountEffect(() => {
    installGlobals(doCopyRef);
    return removeGlobals;
  });

  if (!active) return null;

  return (
    <>
      <div
        ref={highlightRef}
        style={{
          position: 'fixed',
          display: 'none',
          pointerEvents: 'none',
          border: '2px solid var(--color-accent)',
          background: 'var(--color-accent-wash)',
          borderRadius: 4,
          zIndex: 99998,
          transition: 'all 50ms ease-out',
        }}
      />
      <div
        ref={labelRef}
        style={{
          position: 'fixed',
          display: 'none',
          pointerEvents: 'none',
          background: 'var(--color-accent)',
          color: 'var(--color-bg)',
          fontSize: 11,
          fontFamily: 'monospace',
          padding: '2px 6px',
          borderRadius: 4,
          zIndex: 99999,
          whiteSpace: 'nowrap',
        }}
      />
      {toast && (
        <div
          className="flex items-center gap-2"
          style={{
            position: 'fixed',
            bottom: 32,
            right: 16,
            background: 'var(--color-bg-2, #1a1a2e)',
            border: '1px solid var(--color-border, #333)',
            borderRadius: 6,
            padding: '4px 8px',
            zIndex: 100000,
            fontFamily: 'monospace',
            fontSize: 11,
            color: 'var(--color-text-3)',
            pointerEvents: 'none',
          }}
        >
          <span className="text-success" style={{ fontSize: 11 }}>
            ✓ copied
          </span>
          <span
            style={{
              maxWidth: 300,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            }}
          >
            {toast}
          </span>
        </div>
      )}
    </>
  );
}
