import React from 'react';
import { inputStyle, labelStyle, inputCls, labelCls } from '#renderer/styles';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import type { VoiceConfig, FeaturesConfig } from '#renderer/stores/settingsStore';
import { ToggleSwitch } from '#renderer/components/ToggleSwitch';
import { SettingsScout } from '#renderer/components/SettingsScout';

const EMPTY_VOICE: VoiceConfig = {};
const EMPTY_FEATURES: FeaturesConfig = {};

const MODEL_OPTIONS = ['eleven_flash_v2_5', 'eleven_multilingual_v2', 'eleven_turbo_v2_5'];

function VoiceSettings() {
  const voice = useSettingsStore((s) => s.config.voice ?? EMPTY_VOICE);
  const elevenLabsKey = useSettingsStore((s) => s.config.env?.ELEVENLABS_API_KEY ?? '');
  const updateSection = useSettingsStore((s) => s.updateSection);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="sm:col-span-2">
        <label className={labelCls} style={labelStyle}>
          ElevenLabs API Key
        </label>
        <input
          data-testid="voice-elevenlabs-key"
          type="password"
          className={inputCls}
          style={inputStyle}
          value={elevenLabsKey}
          onChange={(e) => {
            updateEnv('ELEVENLABS_API_KEY', e.target.value);
            scheduleSave();
          }}
          placeholder="sk_..."
        />
      </div>
      <div>
        <label className={labelCls} style={labelStyle}>
          Voice ID
        </label>
        <input
          data-testid="voice-voice-id"
          className={inputCls}
          style={inputStyle}
          value={voice.voiceId ?? ''}
          onChange={(e) => {
            updateSection('voice', { voiceId: e.target.value });
            scheduleSave();
          }}
          placeholder="EXAVITQu4vr4xnSDxMaL"
        />
      </div>
      <div>
        <label className={labelCls} style={labelStyle}>
          Model
        </label>
        <select
          data-testid="voice-model"
          className={inputCls}
          style={inputStyle}
          value={voice.model ?? 'eleven_flash_v2_5'}
          onChange={(e) => {
            updateSection('voice', { model: e.target.value });
            save();
          }}
        >
          {MODEL_OPTIONS.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </div>
      <div>
        <label className={labelCls} style={labelStyle}>
          Usage Warning Threshold
        </label>
        <input
          data-testid="voice-usage-threshold"
          type="number"
          min={0}
          max={1}
          step={0.1}
          className={inputCls}
          style={inputStyle}
          value={voice.usageWarningThreshold ?? ''}
          onChange={(e) => {
            updateSection('voice', {
              usageWarningThreshold: e.target.value ? Number(e.target.value) : undefined,
            });
            scheduleSave();
          }}
          placeholder="0.8"
        />
      </div>
      <div>
        <label className={labelCls} style={labelStyle}>
          Session Expiry (days)
        </label>
        <input
          data-testid="voice-session-expiry"
          type="number"
          min={1}
          className={inputCls}
          style={inputStyle}
          value={voice.sessionExpiryDays ?? ''}
          onChange={(e) => {
            updateSection('voice', {
              sessionExpiryDays: e.target.value ? Number(e.target.value) : undefined,
            });
            scheduleSave();
          }}
          placeholder="7"
        />
      </div>
    </div>
  );
}

function LinearSettings() {
  const linearTeam = useSettingsStore((s) => s.config.captain?.linearTeam ?? '');
  const linearKey = useSettingsStore((s) => s.config.env?.LINEAR_API_KEY ?? '');
  const updateSection = useSettingsStore((s) => s.updateSection);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <div>
        <label className={labelCls} style={labelStyle}>
          Linear Team
        </label>
        <input
          data-testid="linear-team"
          className={inputCls}
          style={inputStyle}
          value={linearTeam}
          onChange={(e) => {
            updateSection('captain', { linearTeam: e.target.value });
            scheduleSave();
          }}
          placeholder="e.g. ABR"
        />
      </div>
      <div>
        <label className={labelCls} style={labelStyle}>
          API Key
        </label>
        <input
          data-testid="linear-api-key"
          type="password"
          className={inputCls}
          style={inputStyle}
          value={linearKey}
          onChange={(e) => {
            updateEnv('LINEAR_API_KEY', e.target.value);
            scheduleSave();
          }}
          placeholder="lin_api_..."
        />
      </div>
    </div>
  );
}

interface FlagDef {
  key: keyof FeaturesConfig;
  label: string;
  description: string;
  Settings?: React.FC;
}

const FLAGS: FlagDef[] = [
  {
    key: 'voice',
    label: 'Voice',
    description: 'Voice synthesis via ElevenLabs.',
    Settings: VoiceSettings,
  },
  {
    key: 'decisionJournal',
    label: 'Decision Journal',
    description: 'Learning sessions, pattern recognition, and lesson approval.',
  },
  { key: 'cron', label: 'Cron', description: 'Scheduled cron job management.' },
  {
    key: 'linear',
    label: 'Linear Sync',
    description: 'Import Linear "Todo" issues into captain tasks.',
    Settings: LinearSettings,
  },
  {
    key: 'devMode',
    label: 'Dev Mode',
    description: 'Multi-turn ops sessions and knowledge-base repair.',
  },
  {
    key: 'analytics',
    label: 'Analytics',
    description: 'Task throughput and success metrics dashboard.',
  },
];

export function SettingsExperimental(): React.ReactElement {
  const features = useSettingsStore((s) => s.config.features ?? EMPTY_FEATURES);
  const updateSection = useSettingsStore((s) => s.updateSection);
  const save = useSettingsStore((s) => s.save);

  return (
    <div data-testid="settings-experimental">
      <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
        Experimental
      </h2>
      <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)', marginBottom: 24 }}>
        Alpha features. These may change or be removed at any time.
      </p>

      <div
        style={{
          borderRadius: 'var(--radius-panel)',
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface-1)',
          overflow: 'hidden',
        }}
      >
        {FLAGS.map((flag, i) => {
          const on = !!features[flag.key];
          return (
            <div
              key={flag.key}
              style={i > 0 ? { borderTop: '1px solid var(--color-border-subtle)' } : undefined}
            >
              <div className="flex items-center justify-between" style={{ padding: '14px 20px' }}>
                <div style={{ paddingRight: 16 }}>
                  <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-1)' }}>
                    {flag.label}
                  </h3>
                  <p className="text-xs" style={{ color: 'var(--color-text-3)', marginTop: 2 }}>
                    {flag.description}
                  </p>
                </div>
                <ToggleSwitch
                  testId={`experimental-${flag.key}`}
                  checked={on}
                  onChange={() => {
                    updateSection('features', { [flag.key]: !on });
                    save();
                  }}
                />
              </div>
              {on && flag.Settings && (
                <div
                  style={{
                    padding: '0 20px 16px',
                    borderTop: '1px solid var(--color-border-subtle)',
                    marginLeft: 20,
                    marginRight: 20,
                    paddingTop: 16,
                    marginTop: -1,
                    borderTopStyle: 'dashed',
                  }}
                >
                  <flag.Settings />
                </div>
              )}
            </div>
          );
        })}
      </div>

      <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)', marginTop: 32 }}>
        Scout
      </h2>
      <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)', marginBottom: 16 }}>
        Personalize how Scout selects and explains content.
      </p>
      <div
        style={{
          borderRadius: 'var(--radius-panel)',
          border: '1px solid var(--color-border)',
          background: 'var(--color-surface-1)',
          padding: 20,
        }}
      >
        <SettingsScout />
      </div>
    </div>
  );
}
