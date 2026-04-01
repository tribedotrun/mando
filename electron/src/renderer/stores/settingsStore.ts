import { create } from 'zustand';
import { getErrorMessage } from '#renderer/utils';

// ---- Config shape matching Rust Config struct (spec-config.md) ----
// All field names use camelCase to match serde(rename_all = "camelCase").

export interface ProjectConfig {
  name: string;
  path: string;
  githubRepo?: string | null;
  aliases?: string[];
  hooks?: Record<string, string>;
  workerPreamble?: string;
  scoutSummary?: string;
}

export interface FeaturesConfig {
  voice?: boolean;
  decisionJournal?: boolean;
  cron?: boolean;
  linear?: boolean;
  devMode?: boolean;
  analytics?: boolean;
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
  learnCronExpr?: string;
  tz?: string;
  linearTeam?: string;
  linearCliPath?: string;
  projects?: Record<string, ProjectConfig>;
}

interface CCSelfImproveConfig {
  cooldownS?: number;
  maxRepairsPerHour?: number;
  model?: string;
}

export interface VoiceConfig {
  voiceId?: string;
  model?: string;
  usageWarningThreshold?: number;
  sessionExpiryDays?: number;
}

interface ChannelsConfig {
  telegram?: TelegramConfig;
}

interface ToolsConfig {
  ccSelfImprove?: CCSelfImproveConfig;
}

export interface ScoutInterests {
  high?: string[];
  medium?: string[];
  low?: string[];
  tone?: string;
}

export interface ScoutUserContext {
  role?: string;
  knownDomains?: string[];
  explainDomains?: string[];
}

export interface ScoutConfig {
  interests?: ScoutInterests;
  userContext?: ScoutUserContext;
}

export interface MandoConfig {
  workspace?: string;
  startAtLogin?: boolean;
  features?: FeaturesConfig;
  channels?: ChannelsConfig;
  gateway?: GatewayConfig;
  captain?: CaptainConfig;
  voice?: VoiceConfig;
  scout?: ScoutConfig;
  tools?: ToolsConfig;
  env?: Record<string, string>;
}

// ---- Store ----

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

interface SettingsStore {
  config: MandoConfig;
  loading: boolean;
  loaded: boolean;
  saving: boolean;
  error: string | null;
  saveSuccess: boolean;

  load: () => Promise<void>;
  save: () => Promise<void>;
  scheduleSave: () => void;
  update: (patch: Partial<MandoConfig>) => void;
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
      return;
    }
    const { config } = get();
    set({ saving: true, error: null, saveSuccess: false });
    try {
      if (window.mandoAPI?.saveConfig) {
        await window.mandoAPI.saveConfig(JSON.stringify(config, null, 2));
        set({ saving: false, saveSuccess: true });
        setTimeout(() => set({ saveSuccess: false }), 2000);
      } else {
        set({ saving: false, error: 'Config API not available' });
      }
    } catch (err) {
      set({
        saving: false,
        error: getErrorMessage(err, 'Failed to save config'),
      });
    }
  },

  scheduleSave: () => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      get().save();
    }, 1500);
  },

  update: (patch) => {
    set((s) => ({ config: { ...s.config, ...patch } }));
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
