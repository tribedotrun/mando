import React, { useState } from 'react';
import { cardStyle, inputStyle, labelStyle, inputCls, labelCls } from '#renderer/styles';
import { useSettingsStore } from '#renderer/domains/settings/stores/settingsStore';
import type { ScoutConfig } from '#renderer/domains/settings/stores/settingsStore';

const EMPTY_SCOUT: ScoutConfig = {};
function TagInput({
  label,
  values,
  onChange,
  placeholder,
}: {
  label: string;
  values: string[];
  onChange: (v: string[]) => void;
  placeholder: string;
}) {
  const [draft, setDraft] = useState('');

  const add = () => {
    const trimmed = draft.trim();
    if (trimmed && !values.includes(trimmed)) {
      onChange([...values, trimmed]);
      setDraft('');
    }
  };

  return (
    <div>
      <label className={labelCls} style={labelStyle}>
        {label}
      </label>
      <div className="mb-2 flex flex-wrap gap-2">
        {values.map((v) => (
          <span
            key={v}
            className="inline-flex items-center gap-1 rounded px-2 py-1 text-xs bg-surface-2 text-text-2"
          >
            {v}
            <button
              onClick={() => onChange(values.filter((x) => x !== v))}
              className="ml-0.5 opacity-60 hover:opacity-100"
              style={{ background: 'none', border: 'none', color: 'inherit', cursor: 'pointer' }}
            >
              x
            </button>
          </span>
        ))}
      </div>
      <div className="flex gap-2">
        <input
          className={inputCls}
          style={inputStyle}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && add()}
          placeholder={placeholder}
        />
      </div>
    </div>
  );
}

export function SettingsScout(): React.ReactElement {
  const scout = useSettingsStore((s) => s.config.scout ?? EMPTY_SCOUT);
  const firecrawlKey = useSettingsStore((s) => s.config.env?.FIRECRAWL_API_KEY ?? '');
  const updateSection = useSettingsStore((s) => s.updateSection);
  const updateEnv = useSettingsStore((s) => s.updateEnv);
  const save = useSettingsStore((s) => s.save);
  const scheduleSave = useSettingsStore((s) => s.scheduleSave);

  const interests = scout.interests ?? {};
  const userCtx = scout.userContext ?? {};

  const updateInterests = (patch: Record<string, unknown>, debounce = false) => {
    updateSection('scout', { interests: { ...interests, ...patch } });
    if (debounce) scheduleSave();
    else save();
  };

  const updateUserContext = (patch: Record<string, unknown>, debounce = false) => {
    updateSection('scout', { userContext: { ...userCtx, ...patch } });
    if (debounce) scheduleSave();
    else save();
  };

  return (
    <div data-testid="settings-scout">
      <h2 className="text-lg font-semibold text-text-1" style={{ marginBottom: 24 }}>
        Scout
      </h2>

      <div style={cardStyle}>
        <div className="space-y-4">
          <div>
            <label className={labelCls} style={labelStyle}>
              Firecrawl API Key
            </label>
            <input
              data-testid="scout-firecrawl-key"
              type="password"
              className={inputCls}
              style={inputStyle}
              value={firecrawlKey}
              onChange={(e) => {
                updateEnv('FIRECRAWL_API_KEY', e.target.value);
                scheduleSave();
              }}
              placeholder="fc-..."
            />
            <p className="mt-1 text-xs text-text-3">
              Used for web scraping when processing scout items.
            </p>
          </div>

          <div>
            <label className={labelCls} style={labelStyle}>
              Your Role
            </label>
            <input
              data-testid="scout-role"
              className={inputCls}
              style={inputStyle}
              value={userCtx.role ?? ''}
              onChange={(e) => {
                updateUserContext({ role: e.target.value }, true);
              }}
              placeholder="e.g. Software developer who builds with AI coding agents"
            />
          </div>

          <TagInput
            label="Known Domains (no explanation needed)"
            values={userCtx.knownDomains ?? []}
            onChange={(v) => updateUserContext({ knownDomains: v })}
            placeholder="e.g. Software engineering"
          />

          <TagInput
            label="High Interest Topics"
            values={interests.high ?? []}
            onChange={(v) => updateInterests({ high: v })}
            placeholder="e.g. AI coding tools and workflows"
          />

          <TagInput
            label="Low Interest Topics"
            values={interests.low ?? []}
            onChange={(v) => updateInterests({ low: v })}
            placeholder="e.g. Marketing, growth hacking"
          />
        </div>
      </div>
    </div>
  );
}
