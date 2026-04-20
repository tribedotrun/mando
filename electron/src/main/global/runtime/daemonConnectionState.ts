/**
 * Typed state machine for the daemon connection lifecycle. The reducer is
 * pure; the store holds the current value and dispatches actions. Side
 * effects (scheduleReconnect, invalidateDiscoveryCache, kickstartDaemon)
 * live in `lifecycle.ts` and react to state transitions, not the other way
 * around.
 *
 * Codifies invariant M3 in .claude/skills/s-arch/invariants.md: cross-domain
 * lifecycle state is modeled as a discriminated union + pure reducer.
 */
import type { ConnectionState } from '#main/global/types/lifecycle';

export interface DaemonConnectionSnapshot {
  readonly phase: ConnectionState;
  readonly reconnectAttempts: number;
  readonly reconnectDelay: number;
  readonly healthCheckFailureStreak: number;
}

export type DaemonConnectionAction =
  | { type: 'connected' }
  | { type: 'disconnected' }
  | { type: 'reconnect_failed' }
  | { type: 'health_check_failed' }
  | { type: 'health_check_ok' }
  | { type: 'updating' };

export interface ReduceConfig {
  initialDelay: number;
  maxDelay: number;
}

export function initialSnapshot(cfg: ReduceConfig): DaemonConnectionSnapshot {
  return {
    phase: 'connecting',
    reconnectAttempts: 0,
    reconnectDelay: cfg.initialDelay,
    healthCheckFailureStreak: 0,
  };
}

export function reduce(
  state: DaemonConnectionSnapshot,
  action: DaemonConnectionAction,
  cfg: ReduceConfig,
): DaemonConnectionSnapshot {
  switch (action.type) {
    case 'connected':
      return {
        phase: 'connected',
        reconnectAttempts: 0,
        reconnectDelay: cfg.initialDelay,
        healthCheckFailureStreak: 0,
      };
    case 'disconnected':
      return { ...state, phase: 'disconnected' };
    case 'reconnect_failed':
      return {
        ...state,
        phase: 'disconnected',
        reconnectAttempts: state.reconnectAttempts + 1,
        reconnectDelay: Math.min(state.reconnectDelay * 2, cfg.maxDelay),
      };
    case 'health_check_failed':
      return {
        ...state,
        healthCheckFailureStreak: state.healthCheckFailureStreak + 1,
      };
    case 'health_check_ok':
      return { ...state, healthCheckFailureStreak: 0 };
    case 'updating':
      return { ...state, phase: 'updating' };
  }
}

export interface DaemonConnectionStore {
  get(): DaemonConnectionSnapshot;
  phase(): ConnectionState;
  dispatch(action: DaemonConnectionAction): DaemonConnectionSnapshot;
}

export function createDaemonConnectionStore(cfg: ReduceConfig): DaemonConnectionStore {
  const ref = { current: Object.freeze(initialSnapshot(cfg)) };
  return {
    get: () => ref.current,
    phase: () => ref.current.phase,
    dispatch(action) {
      ref.current = Object.freeze(reduce(ref.current, action, cfg));
      return ref.current;
    },
  };
}
