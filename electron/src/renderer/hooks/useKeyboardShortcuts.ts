import { useRef } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';

/** Returns true when the active element is a text input that should suppress shortcuts. */
function isInputFocused(): boolean {
  const el = document.activeElement;
  if (!el) return false;
  const tag = el.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if ((el as HTMLElement).isContentEditable) return true;
  return false;
}

// ── View handler registry ──
// Only one view is active at a time. It registers a handler that receives
// keys the global handler didn't consume.
type ViewKeyHandler = (key: string, e: KeyboardEvent) => void;
let activeViewHandler: ViewKeyHandler | null = null;

/**
 * Hook for views to register their keyboard handler.
 * The handler receives the raw key string and KeyboardEvent for unhandled keys.
 * Only call this in a component that is conditionally rendered based on the active tab.
 */
export function useViewKeyHandler(handler: ViewKeyHandler): void {
  const ref = useRef(handler);
  ref.current = handler;
  useMountEffect(() => {
    const wrapped: ViewKeyHandler = (key, e) => ref.current(key, e);
    activeViewHandler = wrapped;
    return () => {
      if (activeViewHandler === wrapped) activeViewHandler = null;
    };
  });
}

// ── Global keyboard hook (used in App) ──
interface GlobalKeyboardConfig {
  paletteOpen: boolean;
  shortcutsOpen: boolean;
  showSettings: boolean;
  modalOpen: boolean;
  onNavigate: (tab: 'captain' | 'scout' | 'sessions' | 'cron') => void;
  onTogglePalette: () => void;
  onOpenSettings: () => void;
  onToggleShortcuts: () => void;
}

/**
 * App-level keyboard handler.
 * Handles meta combos (⌘K, ⌘,), G-prefix navigation sequences (G C/D/S),
 * ? (palette), Escape (close overlays), and dispatches remaining keys to the view.
 */
export function useGlobalKeyboard(config: GlobalKeyboardConfig): void {
  const stateRef = useRef(config);
  stateRef.current = config;
  const gPendingRef = useRef(false);
  const gTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useMountEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const s = stateRef.current;

      // ── Meta combos (always active) ──
      if (e.metaKey) {
        if (e.key === 'k') {
          e.preventDefault();
          s.onTogglePalette();
          return;
        }
        if (e.key === ',') {
          e.preventDefault();
          s.onOpenSettings();
          return;
        }
        return;
      }

      // ── Escape (always active, except when input focused) ──
      if (e.key === 'Escape') {
        if (s.paletteOpen) {
          e.preventDefault();
          s.onTogglePalette();
          return;
        }
        if (s.shortcutsOpen) {
          e.preventDefault();
          s.onToggleShortcuts();
          return;
        }
        if (s.showSettings) return;
        activeViewHandler?.('Escape', e);
        return;
      }

      // ── Suppress when input focused or overlays open ──
      if (isInputFocused()) return;
      if (s.paletteOpen || s.shortcutsOpen || s.showSettings || s.modalOpen) return;

      // ── G-prefix sequences ──
      if (gPendingRef.current) {
        gPendingRef.current = false;
        if (gTimerRef.current) {
          clearTimeout(gTimerRef.current);
          gTimerRef.current = null;
        }
        const k = e.key.toLowerCase();
        if (k === 'c') {
          e.preventDefault();
          s.onNavigate('captain');
          return;
        }
        if (k === 'd') {
          e.preventDefault();
          s.onNavigate('scout');
          return;
        }
        if (k === 's') {
          e.preventDefault();
          s.onNavigate('sessions');
          return;
        }
        // Not a valid G-sequence — dispatch this key to the view
      }

      if (e.key === 'g') {
        gPendingRef.current = true;
        gTimerRef.current = setTimeout(() => {
          gPendingRef.current = false;
        }, 500);
        return;
      }

      // ── ? opens shortcut overlay ──
      if (e.key === '?') {
        e.preventDefault();
        s.onToggleShortcuts();
        return;
      }

      // ── Dispatch to active view ──
      activeViewHandler?.(e.key, e);
    };

    window.addEventListener('keydown', handler);
    return () => {
      window.removeEventListener('keydown', handler);
      if (gTimerRef.current) clearTimeout(gTimerRef.current);
    };
  });
}
