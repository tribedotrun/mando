import { create } from 'zustand';
import { getErrorMessage } from '#renderer/utils';

// ---- Config shape matching Rust Config struct (spec-config.md) ----
// All field names use camelCase to match serde(rename_all = "camelCase").

export interface ProjectConfig {
  name: string;
  path: string;
  githubRepo?: string | null;
  logo?: string | null;
  aliases?: string[];
  hooks?: Record<string, string>;
  workerPreamble?: string;
  scoutSummary?: string;
}

export interface FeaturesConfig {
  scout?: boolean;
  setupDismissed?: boolean;
  claudeCodeVerified?: boolean;
}

export interface TelegramConfig {
  enabled?: boolean;
  owner?: string;
}

interface DashboardConfig {
  host?: string;
  port?: number;
}

interface GatewayConfig {
  host?: string;
  port?: number;
  dashboard?: DashboardConfig;
}

export interface CaptainConfig {
  autoSchedule?: boolean;
  tickIntervalS?: number;
  tz?: string;
  defaultTerminalAgent?: 'claude' | 'codex';
  projects?: Record<string, ProjectConfig>;
}

interface ChannelsConfig {
  telegram?: TelegramConfig;
}

interface ScoutInterests {
  high?: string[];
  low?: string[];
}

interface ScoutUserContext {
  role?: string;
  knownDomains?: string[];
  explainDomains?: string[];
}

export interface ScoutConfig {
  interests?: ScoutInterests;
  userContext?: ScoutUserContext;
}

export interface UiConfig {
  openAtLogin?: boolean;
}

export interface MandoConfig {
  workspace?: string;
  ui?: UiConfig;
  features?: FeaturesConfig;
  channels?: ChannelsConfig;
  gateway?: GatewayConfig;
  captain?: CaptainConfig;
  scout?: ScoutConfig;
  env?: Record<string, string>;
}

// ---- Store ----

const SAVE_SUCCESS_DISPLAY_MS = 2000;
const DEBOUNCE_SAVE_MS = 1500;
const ADD_PROJECT_SUCCESS_MS = 2000;

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

interface AddProjectResult {
  ok: boolean;
  name: string;
  path: string;
  githubRepo: string;
}

interface SettingsStore {
  config: MandoConfig;
  loading: boolean;
  loaded: boolean;
  saving: boolean;
  error: string | null;
  saveSuccess: boolean;

  load: () => Promise<void>;
  save: () => Promise<{ ok: boolean; error?: string }>;
  scheduleSave: () => void;
  update: (patch: Partial<MandoConfig>) => void;
  addProject: (project: ProjectConfig) => Promise<AddProjectResult>;
  updateProject: (pathKey: string, project: ProjectConfig) => void;
  removeProject: (pathKey: string) => void;
  updateSection: <K extends keyof MandoConfig>(
    key: K,
    patch: Partial<NonNullable<MandoConfig[K]>>,
  ) => void;
  updateEnv: (key: string, value: string) => void;
  updateTelegram: (patch: Partial<TelegramConfig>) => void;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  config: {},
  loading: false,
  loaded: false,
  saving: false,
  error: null,
  saveSuccess: false,

  load: async () => {
    // Only show loading screen on the first load; subsequent opens use
    // the cached config and refresh silently in the background.
    if (!get().loaded) {
      set({ loading: true, error: null });
    } else {
      set({ error: null });
    }
    try {
      if (window.mandoAPI?.readConfig) {
        const raw = await window.mandoAPI.readConfig();
        const parsed = raw ? JSON.parse(raw) : {};
        set({ config: parsed, loading: false, loaded: true });
      } else {
        set({ loading: false, loaded: true });
      }
    } catch (err) {
      set({
        loading: false,
        error: getErrorMessage(err, 'Failed to load config'),
      });
    }
  },

  save: async () => {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (get().saving) {
      get().scheduleSave();
      return { ok: false, error: 'save already in progress' };
    }
    const { config } = get();
    set({ saving: true, error: null, saveSuccess: false });
    try {
      if (!window.mandoAPI?.saveConfig) {
        const message = 'Config API not available';
        set({ saving: false, error: message });
        return { ok: false, error: message };
      }
      const result = await window.mandoAPI.saveConfig(JSON.stringify(config, null, 2));
      if (result.ok) {
        set({ saving: false, saveSuccess: true });
        setTimeout(() => set({ saveSuccess: false }), SAVE_SUCCESS_DISPLAY_MS);
        return { ok: true };
      }
      const message = `Daemon unreachable, retry (${result.message})`;
      set({ saving: false, error: message });
      return { ok: false, error: message };
    } catch (err) {
      const message = getErrorMessage(err, 'Failed to save config');
      set({ saving: false, error: message });
      return { ok: false, error: message };
    }
  },

  scheduleSave: () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      void get().save();
    }, DEBOUNCE_SAVE_MS);
  },

  update: (patch) => {
    set((s) => ({ config: { ...s.config, ...patch } }));
  },

  addProject: async (project) => {
    // Flush any pending debounced save so reload() doesn't overwrite unsaved edits.
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
      await get().save();
    }
    set({ saving: true, error: null });
    try {
      const result = await window.mandoAPI.addProject(
        JSON.stringify({
          path: project.path,
          name: project.name || undefined,
          aliases: project.aliases?.length ? project.aliases : undefined,
        }),
      );
      // Reload config from daemon to pick up auto-detected fields.
      await get().load();
      set({ saving: false, saveSuccess: true });
      setTimeout(() => set({ saveSuccess: false }), ADD_PROJECT_SUCCESS_MS);
      return result;
    } catch (err) {
      set({ saving: false, error: getErrorMessage(err, 'Failed to add project') });
      throw err;
    }
  },

  updateProject: (pathKey, project) => {
    set((s) => ({
      config: {
        ...s.config,
        captain: {
          ...(s.config.captain || {}),
          projects: { ...(s.config.captain?.projects || {}), [pathKey]: project },
        },
      },
    }));
  },

  removeProject: (pathKey) => {
    set((s) => {
      const projects = { ...(s.config.captain?.projects || {}) };
      delete projects[pathKey];
      return {
        config: {
          ...s.config,
          captain: { ...(s.config.captain || {}), projects },
        },
      };
    });
  },

  updateSection: (key, patch) => {
    set((s) => ({
      config: {
        ...s.config,
        [key]: { ...((s.config[key] as Record<string, unknown>) || {}), ...patch },
      },
    }));
  },

  updateEnv: (key, value) => {
    set((s) => ({
      config: {
        ...s.config,
        env: { ...(s.config.env || {}), [key]: value },
      },
    }));
  },

  updateTelegram: (patch) => {
    set((s) => ({
      config: {
        ...s.config,
        channels: {
          ...s.config.channels,
          telegram: { ...(s.config.channels?.telegram || {}), ...patch },
        },
      },
    }));
  },
}));
