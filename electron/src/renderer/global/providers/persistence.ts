/**
 * Typed persistence boundary.
 *
 * No other file in the renderer references `localStorage`, `sessionStorage`,
 * `window.localStorage`, or `globalThis.localStorage` directly. Enforced by
 * the `architecture/no-direct-localStorage` ESLint rule.
 *
 * Three flavors of accessor:
 *   - `defineSlot(name, owner)` — single named key holding an opaque string.
 *   - `defineJsonSlot(name, schema, owner)` — single named key holding a
 *     Zod-validated JSON value.
 *   - `defineKeyspace(prefix, owner)` — many keys sharing a prefix; suffix
 *     supplied by the caller. Useful for per-task drafts and per-id caches.
 *
 * For third-party libraries (react-resizable-panels) that demand a
 * Storage-shaped object, `createPrefixedStorage` returns the minimum
 * surface they need without exposing raw localStorage.
 */
import type { ZodType } from 'zod';
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';
import { parseJsonTextWith } from '#result';

function backing(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

export interface PersistedSlot {
  readonly key: string;
  read(): string | undefined;
  write(value: string): void;
  clear(): void;
}

export function defineSlot(key: string, owner: string): PersistedSlot {
  return {
    key,
    read(): string | undefined {
      const s = backing();
      if (!s) return undefined;
      try {
        const raw = s.getItem(key);
        return raw === null ? undefined : raw;
      } catch (err) {
        log.warn(`[persistence] read failed for "${key}" (${owner}):`, err);
        return undefined;
      }
    },
    write(value: string): void {
      const s = backing();
      if (!s) return;
      try {
        s.setItem(key, value);
      } catch (err) {
        log.warn(`[persistence] write failed for "${key}" (${owner}):`, err);
      }
    },
    clear(): void {
      const s = backing();
      if (!s) return;
      try {
        s.removeItem(key);
      } catch (err) {
        log.warn(`[persistence] clear failed for "${key}" (${owner}):`, err);
      }
    },
  };
}

export interface PersistedJsonSlot<T> {
  readonly key: string;
  read(): T | undefined;
  write(value: T): void;
  clear(): void;
}

export function defineJsonSlot<T>(
  key: string,
  schema: ZodType<T>,
  owner: string,
): PersistedJsonSlot<T> {
  const slot = defineSlot(key, owner);
  return {
    key,
    read(): T | undefined {
      const raw = slot.read();
      if (raw === undefined) return undefined;
      const result = parseJsonTextWith(raw, schema, `persistence:${key}`);
      if (result.isErr()) {
        log.warn(`[persistence] JSON/schema parse failed for "${key}" (${owner}):`, result.error);
        slot.clear();
        return undefined;
      }
      return result.value;
    },
    write(value: T): void {
      try {
        slot.write(JSON.stringify(value));
      } catch (err) {
        log.warn(`[persistence] JSON.stringify failed for "${key}" (${owner}):`, err);
      }
    },
    clear(): void {
      slot.clear();
    },
  };
}

export interface PersistedKeyspace {
  readonly prefix: string;
  for(suffix: string): PersistedSlot;
}

export function defineKeyspace(prefix: string, owner: string): PersistedKeyspace {
  return {
    prefix,
    for(suffix: string): PersistedSlot {
      return defineSlot(`${prefix}${suffix}`, owner);
    },
  };
}

export interface PersistedJsonKeyspace<T> {
  readonly prefix: string;
  for(suffix: string): PersistedJsonSlot<T>;
}

export function defineJsonKeyspace<T>(
  prefix: string,
  schema: ZodType<T>,
  owner: string,
): PersistedJsonKeyspace<T> {
  return {
    prefix,
    for(suffix: string): PersistedJsonSlot<T> {
      return defineJsonSlot(`${prefix}${suffix}`, schema, owner);
    },
  };
}

/**
 * Minimal Storage-shaped adapter for third-party libraries that need to
 * persist their own state. Keys written through the adapter are namespaced
 * under the given prefix so they don't collide with our typed slots.
 *
 * An empty prefix is an explicit opt-in to the raw, unnamespaced keyspace —
 * used by `usePanelLayout` to preserve pre-refactor panel-layout keys that
 * were already persisted without a prefix. Callers introducing a fresh
 * integration should pass a real prefix (e.g., `'panel:'`).
 */
export interface PrefixedStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
  readonly length: number;
  key(index: number): string | null;
  clear(): void;
}

export function createPrefixedStorage(prefix: string, owner: string): PrefixedStorage {
  const full = (k: string) => `${prefix}${k}`;
  return {
    getItem(key: string): string | null {
      const s = backing();
      if (!s) return null;
      try {
        return s.getItem(full(key));
      } catch (err) {
        log.warn(`[persistence] prefixed getItem "${full(key)}" (${owner}):`, err);
        return null;
      }
    },
    setItem(key: string, value: string): void {
      const s = backing();
      if (!s) return;
      try {
        s.setItem(full(key), value);
      } catch (err) {
        log.warn(`[persistence] prefixed setItem "${full(key)}" (${owner}):`, err);
      }
    },
    removeItem(key: string): void {
      const s = backing();
      if (!s) return;
      try {
        s.removeItem(full(key));
      } catch (err) {
        log.warn(`[persistence] prefixed removeItem "${full(key)}" (${owner}):`, err);
      }
    },
    get length(): number {
      const s = backing();
      if (!s) return 0;
      let n = 0;
      try {
        for (let i = 0; i < s.length; i++) {
          const k = s.key(i);
          if (k !== null && k.startsWith(prefix)) n++;
        }
      } catch (err) {
        log.warn(`[persistence] prefixed length (${owner}):`, err);
      }
      return n;
    },
    key(index: number): string | null {
      const s = backing();
      if (!s) return null;
      let seen = 0;
      try {
        for (let i = 0; i < s.length; i++) {
          const k = s.key(i);
          if (k !== null && k.startsWith(prefix)) {
            if (seen === index) return k.slice(prefix.length);
            seen++;
          }
        }
      } catch (err) {
        log.warn(`[persistence] prefixed key(${index}) (${owner}):`, err);
      }
      return null;
    },
    clear(): void {
      const s = backing();
      if (!s) return;
      try {
        const toRemove: string[] = [];
        for (let i = 0; i < s.length; i++) {
          const k = s.key(i);
          if (k !== null && k.startsWith(prefix)) toRemove.push(k);
        }
        for (const k of toRemove) s.removeItem(k);
      } catch (err) {
        log.warn(`[persistence] prefixed clear (${owner}):`, err);
      }
    },
  };
}

/**
 * Storage-shaped adapter for libraries that already own their JSON encoding but
 * still need boundary validation before the renderer consumes persisted state.
 *
 * Values are stored as raw strings because the library expects the Storage
 * interface, but every read and write is validated as JSON against the schema.
 * Invalid persisted values are dropped so the caller falls back to its default.
 */
export function createJsonStorage<T>(
  prefix: string,
  schema: ZodType<T>,
  owner: string,
): PrefixedStorage {
  const storage = createPrefixedStorage(prefix, owner);

  function where(key: string): string {
    return `persistence:${prefix}${key}`;
  }

  function validate(key: string, value: string): boolean {
    const result = parseJsonTextWith(value, schema, where(key));
    if (result.isOk()) return true;
    log.warn(
      `[persistence] JSON/schema parse failed for "${prefix}${key}" (${owner}):`,
      result.error,
    );
    return false;
  }

  return {
    getItem(key: string): string | null {
      const raw = storage.getItem(key);
      if (raw === null) return null;
      if (validate(key, raw)) return raw;
      storage.removeItem(key);
      return null;
    },
    setItem(key: string, value: string): void {
      if (!validate(key, value)) return;
      storage.setItem(key, value);
    },
    removeItem(key: string): void {
      storage.removeItem(key);
    },
    get length(): number {
      return storage.length;
    },
    key(index: number): string | null {
      return storage.key(index);
    },
    clear(): void {
      storage.clear();
    },
  };
}
