import { round } from '#renderer/global/service/utils';

export interface FiberNode {
  type: string | { name?: string; displayName?: string } | null;
  _debugSource?: { fileName: string; lineNumber: number; columnNumber: number };
  return: FiberNode | null;
  memoizedProps?: Record<string, unknown>;
}

export interface InspectResult {
  component: string;
  file: string | null;
  line: number | null;
  element: string;
  nearbyText: string | null;
  parents: string[];
  props: string[];
  context: Record<string, string>;
}

export function getFiber(el: HTMLElement): FiberNode | null {
  const key = Object.keys(el).find(
    (k) => k.startsWith('__reactFiber$') || k.startsWith('__reactInternalInstance$'),
  );
  return key ? (el as never)[key] : null;
}

export function getComponentName(fiber: FiberNode): string | null {
  if (!fiber.type) return null;
  if (typeof fiber.type === 'string') return null;
  return fiber.type.displayName || fiber.type.name || null;
}

export function findOwnerComponent(fiber: FiberNode): {
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

export function getParentChain(fiber: FiberNode, limit = 5): string[] {
  const chain: string[] = [];
  let current = fiber.return;
  while (current && chain.length < limit) {
    const name = getComponentName(current);
    if (name) chain.push(name);
    current = current.return;
  }
  return chain;
}

export function truncateElement(el: HTMLElement): string {
  const tag = el.tagName.toLowerCase();
  const cls = el.className ? ` class="${String(el.className).slice(0, 60)}"` : '';
  const text = el.textContent?.slice(0, 50) ?? '';
  return `<${tag}${cls}>${text}</${tag}>`;
}

export function findNearbyText(el: HTMLElement): string | null {
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
export function collectPrimitive(ctx: Record<string, string>, key: string, val: unknown): void {
  if (SKIP_PROPS.has(key) || key in ctx || key.startsWith('on')) return;
  if (typeof val === 'string' && val.length > 0 && val.length < 200) {
    ctx[key] = val;
  } else if (typeof val === 'number') {
    ctx[key] = String(val);
  }
}

export function extractContext(fiber: FiberNode): Record<string, string> {
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

export function buildInspectResult(el: HTMLElement): InspectResult | null {
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

export function buildComponentMap(): Array<{
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

// Expose for agent workflow -- called via page.evaluate() from electron-ctl
export function installGlobals(doCopyRef: { current: () => void }) {
  window.__buildComponentMap = buildComponentMap;
  window.__devInspectorCopy = () => doCopyRef.current();
}

export function removeGlobals() {
  delete window.__buildComponentMap;
  delete window.__devInspectorCopy;
}
