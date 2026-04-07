import React, { useState } from 'react';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import type { SetupProgress } from '#renderer/app/Sidebar';

const SETUP_TOTAL = 3;

const STEP_NAMES = ['Install Claude Code', 'Connect Telegram for remote control', 'Add a project'];

/**
 * Compute setup progress from config (no IPC, sidebar-safe).
 * Claude Code detection is async (IPC) so the sidebar can't check it directly.
 * We mark CC as done only after the checklist has validated it (stored in features).
 */
export function useSetupProgress(): SetupProgress | null {
  const config = useSettingsStore((s) => s.config);
  const loaded = useSettingsStore((s) => s.loaded);
  const dismissed = config.features?.setupDismissed ?? false;
  const [hidden, setHidden] = useState(false);
  const timerRef = React.useRef<ReturnType<typeof setTimeout>>(undefined);

  const hasProject = Object.keys(config.captain?.projects ?? {}).length > 0;
  const done = [
    !!config.features?.claudeCodeVerified,
    !!(config.channels?.telegram?.enabled && config.env?.TELEGRAM_MANDO_BOT_TOKEN),
    hasProject,
  ];
  const completed = done.filter(Boolean).length;
  const allDone = completed === SETUP_TOTAL;

  // Schedule auto-hide 3s after reaching 100%. Zustand selector re-renders
  // trigger this on every config change, so the timer is set exactly once.
  if (allDone && !hidden && timerRef.current === undefined) {
    timerRef.current = setTimeout(() => setHidden(true), 3000);
  }
  if (!allDone && timerRef.current !== undefined) {
    clearTimeout(timerRef.current);
    timerRef.current = undefined;
  }

  if (!loaded || dismissed || hidden) return null;

  const firstIncomplete = done.findIndex((d) => !d);
  const stepLabel = firstIncomplete >= 0 ? STEP_NAMES[firstIncomplete] : 'All done!';
  return { completed, total: SETUP_TOTAL, currentStep: stepLabel };
}
