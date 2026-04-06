/**
 * Shared helpers for Zustand stores.
 */

/**
 * Create a fetch-generation guard so a store can drop stale responses
 * when multiple fetches are in flight. Each store keeps its own counter.
 *
 * Usage:
 *   const gen = createFetchGenerationGuard();
 *   // inside fetch():
 *   const n = gen.next();
 *   const data = await api();
 *   if (!gen.isLatest(n)) return;
 */
export function createFetchGenerationGuard(): {
  next: () => number;
  isLatest: (gen: number) => boolean;
} {
  let current = 0;
  return {
    next: () => ++current,
    isLatest: (gen) => gen === current,
  };
}
