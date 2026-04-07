import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import Markdown from 'react-markdown';
import {
  fetchScoutItem,
  fetchScoutArticle,
  actOnScoutItem,
  publishScoutTelegraph,
  fetchScoutItemSessions,
} from '#renderer/domains/scout/hooks/useApi';
import type { ScoutItem } from '#renderer/types';
import { getErrorMessage } from '#renderer/utils';
import { useProjects } from '#renderer/domains/settings';
import log from '#renderer/logger';
import { Button } from '#renderer/components/ui/button';
import { Input } from '#renderer/components/ui/input';
import { Badge } from '#renderer/components/ui/badge';
import { Separator } from '#renderer/components/ui/separator';
import { Skeleton } from '#renderer/components/ui/skeleton';
import { Card, CardContent } from '#renderer/components/ui/card';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/components/ui/collapsible';
import { Combobox } from '#renderer/components/ui/combobox';

interface Props {
  itemId: number;
  onBack: () => void;
  onAsk: () => void;
  qaOpen?: boolean;
}

// Parent should render with key={itemId} so this component remounts on item change
export function ScoutReader({ itemId, onBack, onAsk, qaOpen }: Props): React.ReactElement {
  const [summaryOpen, setSummaryOpen] = useState(true);
  const [actOpen, setActOpen] = useState(false);
  const [actProject, setActProject] = useState('');
  const [actPrompt, setActPrompt] = useState('');
  const [acting, setActing] = useState(false);
  const [actResult, setActResult] = useState<string | null>(null);
  const [publishing, setPublishing] = useState(false);
  const [publishedUrl, setPublishedUrl] = useState<string | null>(null);
  const [sessionsOpen, setSessionsOpen] = useState(false);

  const projects = useProjects();

  // Derive effective project: auto-select when exactly one exists
  const effectiveActProject = actProject || (projects.length === 1 ? projects[0] : '');

  const itemQuery = useQuery({
    queryKey: ['scout', 'item', itemId],
    queryFn: () => fetchScoutItem(itemId),
  });

  const articleQuery = useQuery({
    queryKey: ['scout', 'article', itemId],
    queryFn: () => fetchScoutArticle(itemId),
  });

  const item: ScoutItem | null = itemQuery.data ?? null;
  const loading = itemQuery.isLoading;
  const error = itemQuery.error ? getErrorMessage(itemQuery.error, 'Failed') : null;
  const displayTitle =
    item?.title || (item?.status === 'pending' ? 'Pending processing\u2026' : 'Untitled');
  const article = articleQuery.data?.article ?? null;
  const telegraphUrl =
    publishedUrl ?? articleQuery.data?.telegraphUrl ?? item?.telegraphUrl ?? null;
  const articleLoading = articleQuery.isLoading;
  const sessionsQuery = useQuery({
    queryKey: ['scout', 'sessions', itemId],
    queryFn: () => fetchScoutItemSessions(itemId),
    enabled: sessionsOpen,
  });

  const handleAct = async () => {
    if (!effectiveActProject) return;
    setActing(true);
    setActResult(null);
    try {
      const res = await actOnScoutItem(itemId, effectiveActProject, actPrompt || undefined);
      if (res.skipped) {
        setActResult(`Skipped: ${res.reason}`);
      } else {
        setActResult(`Created task: ${res.title}`);
      }
    } catch (e) {
      log.warn('[ScoutReader] actOnScoutItem failed', { itemId, err: e });
      setActResult(`Error: ${getErrorMessage(e, 'unknown')}`);
    } finally {
      setActing(false);
    }
  };

  const handlePublish = async () => {
    if (telegraphUrl) {
      window.open(telegraphUrl, '_blank', 'noopener,noreferrer');
      return;
    }
    setPublishing(true);
    try {
      const res = await publishScoutTelegraph(itemId);
      setPublishedUrl(res.url);
      window.open(res.url, '_blank', 'noopener,noreferrer');
    } catch (e) {
      log.warn('[ScoutReader] publishScoutTelegraph failed', { itemId, err: e });
      setActResult(`Publish error: ${getErrorMessage(e, 'unknown')}`);
    } finally {
      setPublishing(false);
    }
  };

  if (loading) {
    return (
      <div data-testid="scout-reader" className="mx-auto max-w-[720px] space-y-4 py-8">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    );
  }

  if (error || !item) {
    return (
      <div data-testid="scout-reader">
        <Button variant="ghost" size="sm" onClick={onBack} className="mb-3">
          &larr; Back
        </Button>
        <div className="text-xs text-destructive">{error ?? `Item #${itemId} not found`}</div>
      </div>
    );
  }

  return (
    <div data-testid="scout-reader" className={qaOpen ? '' : 'mx-auto max-w-[720px]'}>
      {/* Nav bar */}
      <div
        className="sticky top-0 z-10 mb-4 flex items-center gap-2 py-2 backdrop-blur-sm"
        style={{
          background: 'color-mix(in srgb, var(--background) 90%, transparent)',
        }}
      >
        <Button variant="ghost" size="sm" onClick={onBack}>
          &larr; Back
        </Button>
        <span className="max-w-[300px] truncate text-xs text-muted-foreground">{displayTitle}</span>
        <div className="ml-auto flex items-center gap-1">
          <Button variant={qaOpen ? 'secondary' : 'ghost'} size="sm" onClick={onAsk}>
            Ask
          </Button>
          {(item.status === 'processed' ||
            item.status === 'saved' ||
            item.status === 'archived') && (
            <Button
              variant={actOpen ? 'secondary' : 'ghost'}
              size="sm"
              onClick={() => {
                setActOpen(!actOpen);
                setActResult(null);
              }}
            >
              Act
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={handlePublish} disabled={publishing}>
            {publishing ? 'Publishing...' : telegraphUrl ? 'Open article' : 'Publish'}
          </Button>
          <Button
            variant={sessionsOpen ? 'secondary' : 'ghost'}
            size="sm"
            onClick={() => setSessionsOpen((value) => !value)}
          >
            Sessions
          </Button>
          <Button variant="ghost" size="sm" asChild>
            <a href={item.url} target="_blank" rel="noopener noreferrer">
              Source
            </a>
          </Button>
        </div>
      </div>

      {/* Act form */}
      {actOpen && (
        <ScoutActForm
          projects={projects}
          actProject={effectiveActProject}
          setActProject={setActProject}
          actPrompt={actPrompt}
          setActPrompt={setActPrompt}
          acting={acting}
          actResult={actResult}
          onAct={handleAct}
        />
      )}

      {sessionsOpen && (
        <Card className="mb-5 py-3">
          <CardContent className="space-y-2 px-4">
            <div className="text-label text-muted-foreground">Scout sessions</div>
            {sessionsQuery.isLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
              </div>
            ) : sessionsQuery.error ? (
              <div className="text-xs text-destructive">
                {getErrorMessage(sessionsQuery.error, 'Failed to load sessions')}
              </div>
            ) : (sessionsQuery.data?.length ?? 0) === 0 ? (
              <div className="text-xs text-muted-foreground">No linked sessions.</div>
            ) : (
              <div className="space-y-2">
                {sessionsQuery.data?.map((session) => (
                  <div
                    key={session.session_id}
                    className="flex items-center justify-between gap-3 rounded-md bg-muted px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-xs font-medium text-muted-foreground">
                        {session.session_id}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        {session.caller} · {session.status}
                      </div>
                    </div>
                    <div className="text-[11px] text-muted-foreground">{session.created_at}</div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      {/* Title block */}
      <div className="mb-5">
        <h1 className="mb-2 text-lg font-semibold leading-snug text-foreground">{displayTitle}</h1>
        <div className="flex items-center gap-3 font-mono text-xs text-muted-foreground">
          <Badge variant="outline" className="text-[10px]">
            {item.item_type ?? 'blog'}
          </Badge>
          {item.relevance != null && (
            <span>
              R:{item.relevance} Q:{item.quality}
            </span>
          )}
          {item.source_name && <span>{item.source_name}</span>}
          {item.date_published && <span>{item.date_published}</span>}
        </div>
      </div>

      {/* Summary */}
      {item.summary && (
        <ScoutSummary
          summary={item.summary}
          summaryOpen={summaryOpen}
          onToggle={() => setSummaryOpen(!summaryOpen)}
        />
      )}

      <Separator className="my-4" />

      {/* Article */}
      <div>
        {articleLoading && (
          <div className="space-y-3 py-8">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-4 w-4/6" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
          </div>
        )}
        {article && (
          <div className="prose-scout text-sm leading-relaxed text-foreground">
            <Markdown>{article}</Markdown>
          </div>
        )}
        {!article && !articleLoading && (
          <div className="py-8 text-center text-xs text-muted-foreground">
            No article content. Process the item first.
          </div>
        )}
      </div>
    </div>
  );
}

function ScoutSummary({
  summary,
  summaryOpen,
  onToggle,
}: {
  summary: string;
  summaryOpen: boolean;
  onToggle: () => void;
}): React.ReactElement {
  return (
    <Collapsible open={summaryOpen} onOpenChange={onToggle} className="mb-5">
      <CollapsibleTrigger className="flex items-center gap-2 text-label text-muted-foreground">
        <span className="text-[0.6rem]">{summaryOpen ? '\u25BC' : '\u25B6'}</span>
        Process Summary
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div
          className="prose-scout mt-2 border-l-2 pl-4 text-xs leading-relaxed text-foreground"
          style={{
            borderColor: 'color-mix(in srgb, var(--primary) 30%, transparent)',
          }}
        >
          <Markdown>{summary}</Markdown>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function ScoutActForm({
  projects,
  actProject,
  setActProject,
  actPrompt,
  setActPrompt,
  acting,
  actResult,
  onAct,
}: {
  projects: string[];
  actProject: string;
  setActProject: (v: string) => void;
  actPrompt: string;
  setActPrompt: (v: string) => void;
  acting: boolean;
  actResult: string | null;
  onAct: () => void;
}): React.ReactElement {
  return (
    <Card className="mb-5 py-3">
      <CardContent className="flex flex-col gap-3 px-4">
        <div className="flex items-center gap-2">
          {projects.length > 1 && (
            <Combobox
              value={actProject}
              onValueChange={setActProject}
              options={projects.map((p) => ({ value: p, label: p }))}
              placeholder="Select project..."
              searchPlaceholder="Search projects..."
              emptyText="No projects found."
              className="shrink-0 text-xs"
            />
          )}
          {projects.length === 1 && <Badge variant="secondary">{projects[0]}</Badge>}
        </div>
        <div className="flex items-center gap-2">
          <Input
            type="text"
            value={actPrompt}
            onChange={(e) => setActPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && actProject && !acting) onAct();
            }}
            placeholder="What should the task focus on? (optional)"
            className="h-8 min-w-0 flex-1 text-xs"
          />
          <Button size="sm" onClick={onAct} disabled={!actProject || acting}>
            {acting ? 'Creating...' : 'Create Task'}
          </Button>
        </div>
        {actResult && <div className="text-xs text-muted-foreground">{actResult}</div>}
      </CardContent>
    </Card>
  );
}
