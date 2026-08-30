import { createContext, use } from 'react';

export type Tab = 'captain' | 'scout' | 'sessions';

export interface SetupProgress {
  completed: number;
  total: number;
  currentStep: string;
}

export interface SidebarActions {
  // Navigation
  changeTab: (tab: Tab) => void;
  openTask: (taskId: number, workbenchId?: number) => void;
  openWorkbench: (workbench: { id?: number; worktree: string }) => void;
  openSettings: () => void;
  newTask: () => void;
  goBack: () => void;
  goForward: () => void;
  toggleSidebar: () => void;
  filterByProject: (project: string | null) => void;

  // Workbench operations
  archiveWorkbench: (id: number) => void;
  unarchiveWorkbench: (id: number) => void;
  pinWorkbench: (id: number) => void;
  unpinWorkbench: (id: number) => void;
  renameWorkbench: (id: number, title: string) => void;
  openWorkbenchInFinder: (worktree: string) => void;
  copyWorkbenchPath: (worktree: string) => void;

  // Project operations
  addProject: () => void;
  renameProject: (oldName: string, newName: string) => Promise<void>;
  removeProject: (name: string) => Promise<void>;

  // Setup
  toggleSetup: () => void;
  dismissSetup: () => void;
}

export interface SidebarState {
  activeTab: Tab;
  projectFilter: string | null;
  /** Worktree path of the workbench the user is currently viewing, if any. */
  activeWorktreePath: string | null;
  activeTaskId: number | null;
  setupProgress: SetupProgress | null;
  setupActive: boolean;
}

export interface SidebarContextValue {
  state: SidebarState;
  actions: SidebarActions;
}

export const SidebarContext = createContext<SidebarContextValue | null>(null);

export function useSidebar(): SidebarContextValue {
  const ctx = use(SidebarContext);
  if (!ctx) {
    // invariant: SidebarContext must be wrapped by SidebarProvider; missing means a tree mistake
    throw new Error('useSidebar must be used within a SidebarProvider');
  }
  return ctx;
}
