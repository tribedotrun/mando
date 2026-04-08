import React, { useState } from 'react';
import { Card, CardContent } from '#renderer/components/ui/card';
import { Input } from '#renderer/components/ui/input';
import { Label } from '#renderer/components/ui/label';
import { Badge } from '#renderer/components/ui/badge';
import { Button } from '#renderer/components/ui/button';
import {
  useSettingsStore,
  type ScoutConfig,
} from '#renderer/domains/settings/stores/settingsStore';

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
      <Label className="mb-1.5 text-xs text-muted-foreground">{label}</Label>
      <div className="mb-2 flex flex-wrap gap-2">
        {values.map((v) => (
          <Badge key={v} variant="secondary" className="gap-1 text-xs">
            {v}
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={() => onChange(values.filter((x) => x !== v))}
              className="ml-0.5 h-3 w-3 opacity-60 hover:opacity-100"
            >
              x
            </Button>
          </Badge>
        ))}
      </div>
      <div className="flex gap-2">
        <Input
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
    else void save();
  };

  const updateUserContext = (patch: Record<string, unknown>, debounce = false) => {
    updateSection('scout', { userContext: { ...userCtx, ...patch } });
    if (debounce) scheduleSave();
    else void save();
  };

  return (
    <div data-testid="settings-scout">
      <h2 className="mb-6 text-lg font-semibold text-foreground">Scout</h2>

      <Card className="py-4">
        <CardContent className="space-y-4">
          <div>
            <Label className="mb-1.5 text-xs text-muted-foreground">Firecrawl API Key</Label>
            <Input
              data-testid="scout-firecrawl-key"
              type="password"
              value={firecrawlKey}
              onChange={(e) => {
                updateEnv('FIRECRAWL_API_KEY', e.target.value);
                scheduleSave();
              }}
              placeholder="fc-..."
            />
            <p className="mt-1 text-xs text-muted-foreground">
              Used for web scraping when processing scout items.
            </p>
          </div>

          <div>
            <Label className="mb-1.5 text-xs text-muted-foreground">Your Role</Label>
            <Input
              data-testid="scout-role"
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
        </CardContent>
      </Card>
    </div>
  );
}
