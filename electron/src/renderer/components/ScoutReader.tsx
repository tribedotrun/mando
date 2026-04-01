import React, { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import Markdown from 'react-markdown';
import {
  fetchScoutItem,
  fetchScoutArticle,
  actOnScoutItem,
  fetchHealth,
  publishScoutTelegraph,
  fetchScoutItemSessions,
} from '#renderer/api';
import type { ScoutItem } from '#renderer/types';
import { getErrorMessage } from '#renderer/utils';

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

  const projectsQuery = useQuery({
    queryKey: ['status', 'projects'],
    queryFn: async () => {
      const s = await fetchHealth();
      return s.projects ?? [];
    },
  });

  const projects = projectsQuery.data ?? [];
  const projectsError = projectsQuery.error ? 'Failed to load projects' : null;

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
      setActResult(`Publish error: ${getErrorMessage(e, 'unknown')}`);
    } finally {
      setPublishing(false);
    }
  };

  if (loading) {
    return (
      <div data-testid="scout-reader" className="flex items-center gap-2 py-16 justify-center">
        <span className="text-xs" style={{ color: 'var(--color-text-3)' }}>
          Loading...
        </span>
      </div>
    );
  }

  if (error || !item) {
    return (
      <div data-testid="scout-reader">
        <button
          onClick={onBack}
          className="mb-3 rounded px-3 py-1 text-xs"
          style={{ color: 'var(--color-text-2)' }}
        >
          &larr; Back
        </button>
        <div className="text-xs" style={{ color: 'var(--color-error)' }}>
          {error ?? `Item #${itemId} not found`}
        </div>
      </div>
    );
  }

  return (
    <div data-testid="scout-reader" className={qaOpen ? '' : 'mx-auto max-w-[720px]'}>
      {/* Nav bar */}
      <div
        className="sticky top-0 z-10 mb-4 flex items-center gap-2 py-2 backdrop-blur-sm border-b"
        style={{
          background: 'color-mix(in srgb, var(--color-bg) 90%, transparent)',
          borderColor: 'var(--color-surface-2)',
        }}
      >
        <button
          onClick={onBack}
          className="rounded px-3 py-1 text-xs"
          style={{ color: 'var(--color-text-2)' }}
        >
          &larr; Back
        </button>
        <span className="text-xs truncate max-w-[300px]" style={{ color: 'var(--color-text-3)' }}>
          {displayTitle}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <button
            onClick={onAsk}
            className="rounded px-3 py-1 text-xs"
            style={
              qaOpen
                ? {
                    background: 'var(--color-accent-wash)',
                    color: 'var(--color-accent)',
                  }
                : { color: 'var(--color-text-2)' }
            }
          >
            Ask
          </button>
          {(item.status === 'processed' ||
            item.status === 'saved' ||
            item.status === 'archived') && (
            <button
              onClick={() => {
                setActOpen(!actOpen);
                setActResult(null);
              }}
              className="rounded px-3 py-1 text-xs"
              style={
                actOpen
                  ? {
                      background: 'var(--color-accent-wash)',
                      color: 'var(--color-accent)',
                    }
                  : { color: 'var(--color-text-2)' }
              }
            >
              Act
            </button>
          )}
          <button
            onClick={handlePublish}
            className="rounded px-3 py-1 text-xs"
            disabled={publishing}
            style={{ color: 'var(--color-text-2)' }}
          >
            {publishing ? 'Publishing…' : telegraphUrl ? 'Open article' : 'Publish'}
          </button>
          <button
            onClick={() => setSessionsOpen((value) => !value)}
            className="rounded px-3 py-1 text-xs"
            style={
              sessionsOpen
                ? { background: 'var(--color-accent-wash)', color: 'var(--color-accent)' }
                : { color: 'var(--color-text-2)' }
            }
          >
            Sessions
          </button>
          <a
            href={item.url}
            target="_blank"
            rel="noopener noreferrer"
            className="rounded px-3 py-1 text-xs"
            style={{ color: 'var(--color-text-2)' }}
          >
            Source
          </a>
        </div>
      </div>

      {/* Act form — directly below nav bar */}
      {actOpen && (
        <ScoutActForm
          projects={projects}
          projectsError={projectsError}
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
        <div
          className="mb-5 rounded-lg border p-3"
          style={{
            borderColor: 'var(--color-border)',
            background: 'var(--color-surface-1)',
          }}
        >
          <div
            className="mb-2 text-[11px] font-semibold uppercase tracking-wider"
            style={{ color: 'var(--color-text-4)' }}
          >
            Scout sessions
          </div>
          {sessionsQuery.isLoading ? (
            <div className="text-xs" style={{ color: 'var(--color-text-4)' }}>
              Loading sessions…
            </div>
          ) : sessionsQuery.error ? (
            <div className="text-xs" style={{ color: 'var(--color-error)' }}>
              {getErrorMessage(sessionsQuery.error, 'Failed to load sessions')}
            </div>
          ) : (sessionsQuery.data?.length ?? 0) === 0 ? (
            <div className="text-xs" style={{ color: 'var(--color-text-4)' }}>
              No linked sessions.
            </div>
          ) : (
            <div className="space-y-2">
              {sessionsQuery.data?.map((session) => (
                <div
                  key={session.session_id}
                  className="flex items-center justify-between gap-3 rounded border px-3 py-2"
                  style={{
                    borderColor: 'var(--color-border-subtle)',
                    background: 'var(--color-surface-2)',
                  }}
                >
                  <div className="min-w-0 flex-1">
                    <div
                      className="truncate text-xs font-medium"
                      style={{ color: 'var(--color-text-2)' }}
                    >
                      {session.session_id}
                    </div>
                    <div className="text-[11px]" style={{ color: 'var(--color-text-4)' }}>
                      {session.caller} · {session.status}
                    </div>
                  </div>
                  <div className="text-[11px]" style={{ color: 'var(--color-text-4)' }}>
                    {session.created_at}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Title block */}
      <div className="mb-5">
        <h1
          className="text-lg font-semibold leading-snug mb-2"
          style={{ color: 'var(--color-text-1)' }}
        >
          {displayTitle}
        </h1>
        <div
          className="flex items-center gap-3 font-mono text-xs"
          style={{ color: 'var(--color-text-3)' }}
        >
          <span
            className="rounded border px-1.5 py-0.5 text-[0.6rem] uppercase"
            style={{ borderColor: 'var(--color-border)' }}
          >
            {item.item_type ?? 'blog'}
          </span>
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

      {/* Article */}
      <div>
        {articleLoading && (
          <div className="text-xs text-center py-12" style={{ color: 'var(--color-text-3)' }}>
            Loading article...
          </div>
        )}
        {article && (
          <div
            className="prose-scout text-sm leading-relaxed"
            style={{ color: 'var(--color-text-1)' }}
          >
            <Markdown>{article}</Markdown>
          </div>
        )}
        {!article && !articleLoading && (
          <div className="text-xs text-center py-8" style={{ color: 'var(--color-text-3)' }}>
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
    <div className="mb-5">
      <button
        onClick={onToggle}
        className="flex items-center gap-1.5 mb-2 text-[0.7rem] uppercase tracking-wider"
        style={{ color: 'var(--color-text-3)' }}
      >
        <span className="text-[0.6rem]">{summaryOpen ? '\u25BC' : '\u25B6'}</span>
        Process Summary
      </button>
      {summaryOpen && (
        <div
          className="border-l-2 pl-4 text-xs leading-relaxed prose-scout"
          style={{
            borderColor: 'color-mix(in srgb, var(--color-accent) 30%, transparent)',
            color: 'var(--color-text-1)',
          }}
        >
          <Markdown>{summary}</Markdown>
        </div>
      )}
    </div>
  );
}

function ScoutActForm({
  projects,
  projectsError,
  actProject,
  setActProject,
  actPrompt,
  setActPrompt,
  acting,
  actResult,
  onAct,
}: {
  projects: string[];
  projectsError: string | null;
  actProject: string;
  setActProject: (v: string) => void;
  actPrompt: string;
  setActPrompt: (v: string) => void;
  acting: boolean;
  actResult: string | null;
  onAct: () => void;
}): React.ReactElement {
  return (
    <div
      className="mb-5 rounded-lg p-4"
      style={{ background: 'var(--color-surface-2)', border: '1px solid var(--color-border)' }}
    >
      {projectsError ? (
        <div className="text-xs" style={{ color: 'var(--color-error)' }}>
          {projectsError}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            {projects.length > 1 && (
              <select
                value={actProject}
                onChange={(e) => setActProject(e.target.value)}
                className="shrink-0 rounded-md px-2.5 py-1.5 text-xs"
                style={{
                  border: '1px solid var(--color-border)',
                  background: 'var(--color-surface-3)',
                  color: 'var(--color-text-1)',
                }}
              >
                <option value="">Select project...</option>
                {projects.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
            )}
            {projects.length === 1 && (
              <span
                className="shrink-0 rounded-md px-2.5 py-1.5 text-xs font-medium"
                style={{ color: 'var(--color-text-2)', background: 'var(--color-surface-3)' }}
              >
                {projects[0]}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={actPrompt}
              onChange={(e) => setActPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && actProject && !acting) onAct();
              }}
              placeholder="What should the task focus on? (optional)"
              className="min-w-0 flex-1 rounded-md px-2.5 py-1.5 text-xs focus:outline-none"
              style={{
                border: '1px solid var(--color-border)',
                background: 'var(--color-surface-3)',
                color: 'var(--color-text-1)',
              }}
            />
            <button
              onClick={onAct}
              disabled={!actProject || acting}
              className="shrink-0 rounded-md px-4 py-1.5 text-xs font-medium text-white disabled:opacity-40"
              style={{ background: 'var(--color-accent)' }}
            >
              {acting ? 'Creating...' : 'Create Task'}
            </button>
          </div>
          {actResult && (
            <div className="text-xs" style={{ color: 'var(--color-text-2)' }}>
              {actResult}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
