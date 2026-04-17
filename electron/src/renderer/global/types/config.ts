// Config types matching Rust Config struct (camelCase serde)

export interface ProjectConfig {
  name: string;
  path: string;
  githubRepo?: string | null;
  logo?: string | null;
  aliases?: string[];
  hooks?: Record<string, string>;
  workerPreamble?: string;
  scoutSummary?: string;
  checkCommand?: string;
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

export interface CaptainConfig {
  autoSchedule?: boolean;
  autoMerge?: boolean;
  maxConcurrentWorkers?: number;
  tickIntervalS?: number;
  tz?: string;
  defaultTerminalAgent?: 'claude' | 'codex';
  claudeTerminalArgs?: string;
  codexTerminalArgs?: string;
  projects?: Record<string, ProjectConfig>;
}

export interface ScoutConfig {
  interests?: { high?: string[]; low?: string[] };
  userContext?: { role?: string; knownDomains?: string[]; explainDomains?: string[] };
}

export interface UiConfig {
  openAtLogin?: boolean;
}

export interface MandoConfig {
  workspace?: string;
  ui?: UiConfig;
  features?: FeaturesConfig;
  channels?: { telegram?: TelegramConfig };
  gateway?: { host?: string; port?: number; dashboard?: { host?: string; port?: number } };
  captain?: CaptainConfig;
  scout?: ScoutConfig;
  env?: Record<string, string>;
}
