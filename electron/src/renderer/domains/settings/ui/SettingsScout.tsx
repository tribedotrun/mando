import React, { useState } from 'react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Input } from '#renderer/global/ui/primitives/input';
import { Label } from '#renderer/global/ui/primitives/label';
import { Badge } from '#renderer/global/ui/primitives/badge';
import { Button } from '#renderer/global/ui/primitives/button';
import { useConfig, useConfigPatch } from '#renderer/domains/settings/runtime/hooks';
import {
  scoutInterestsPatch,
  scoutUserContextPatch,
  envPatch,
} from '#renderer/global/service/configPatches';
import type { ScoutConfig } from '#renderer/global/types';

const EMPTY_SCOUT: ScoutConfig = Object.freeze({});

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
  const { data: config } = useConfig();
  const { save, debouncedSave } = useConfigPatch();
  const scout = config?.scout ?? EMPTY_SCOUT;
  const firecrawlKey = config?.env?.FIRECRAWL_API_KEY ?? '';

  const interests = scout.interests ?? {};
  const userCtx = scout.userContext ?? {};

  const updateInterests = (patch: Record<string, unknown>, debounce = false) => {
    (debounce ? debouncedSave : save)(scoutInterestsPatch(patch));
  };

  const updateUserContext = (patch: Record<string, unknown>, debounce = false) => {
    (debounce ? debouncedSave : save)(scoutUserContextPatch(patch));
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
                debouncedSave(envPatch({ FIRECRAWL_API_KEY: e.target.value }));
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
