import React, { useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';

/**
 * Parsed section from a PR description.
 * heading is empty string for preamble (text before the first ## heading).
 */
interface Section {
  heading: string;
  content: string;
}

type GroupKey = 'main' | 'prSummary' | 'details' | 'full';

/** Strip the Devin review badge and trailing HRs before it. */
function stripBadge(text: string): string {
  return text
    .replace(/<!-- devin-review-badge-begin -->[\s\S]*?<!-- devin-review-badge-end -->/g, '')
    .replace(/<!-- pr-summary-head:.*?-->/g, '')
    .replace(/(\n\s*---\s*)+\s*$/g, '')
    .trim();
}

/** Split PR body into sections at `## ` (h2) headings. */
function parseSections(raw: string): Section[] {
  const text = stripBadge(raw);
  const lines = text.split('\n');
  const sections: Section[] = [];
  let heading = '';
  let buf: string[] = [];

  const flush = () => {
    const content = buf.join('\n').trim();
    if (content) sections.push({ heading, content });
  };

  let inCodeBlock = false;
  for (const line of lines) {
    if (line.trimStart().startsWith('```')) inCodeBlock = !inCodeBlock;
    const m = !inCodeBlock && line.match(/^##\s+(.+)/);
    if (m) {
      flush();
      heading = m[1].trim();
      buf = [];
    } else {
      buf.push(line);
    }
  }
  flush();
  return sections;
}

/** Classify a section heading into one of the display groups. */
function classifySection(heading: string): Exclude<GroupKey, 'full'> {
  if (!heading) return 'main';
  const h = heading.toLowerCase();
  if (/pr\s*summary/.test(h)) return 'prSummary';
  if (/checklist|reviewer/i.test(h)) return 'details';
  // Everything else goes to main — only explicit checklist headings become their own tab.
  return 'main';
}

/**
 * For the PR Summary section, split out the Reviewer Checklist (### heading)
 * into the details group so it collapses separately.
 */
function splitChecklist(content: string): { summary: string; checklist: string | null } {
  const idx = content.indexOf('### Reviewer Checklist');
  if (idx === -1) return { summary: content, checklist: null };
  return {
    summary: content.slice(0, idx).trim(),
    checklist: content.slice(idx).trim(),
  };
}

interface Tab {
  key: GroupKey;
  label: string;
}

interface Props {
  text: string;
  onRefresh?: () => void;
  refreshing?: boolean;
}

export function PrSections({ text, onRefresh, refreshing }: Props): React.ReactElement | null {
  const cleaned = stripBadge(text);
  const sections = parseSections(text);

  const groups: Record<Exclude<GroupKey, 'full'>, Section[]> = {
    main: [],
    prSummary: [],
    details: [],
  };
  for (const s of sections) {
    const key = classifySection(s.heading);
    if (key === 'prSummary') {
      const { summary, checklist } = splitChecklist(s.content);
      if (summary) groups.prSummary.push({ heading: s.heading, content: summary });
      if (checklist)
        groups.details.push({
          heading: 'Reviewer Checklist',
          content: checklist.replace(/^### Reviewer Checklist\n?/, ''),
        });
    } else {
      groups[key].push(s);
    }
  }

  if (sections.length === 0) {
    return <span className="text-[12px] italic text-text-3">No description</span>;
  }

  // Build tabs — only include groups that have content, always add Full
  const tabs: Tab[] = [];
  if (groups.main.length > 0) tabs.push({ key: 'main', label: 'Summary' });
  if (groups.prSummary.length > 0) tabs.push({ key: 'prSummary', label: 'Diagram' });
  if (groups.details.length > 0) tabs.push({ key: 'details', label: 'Checklist' });

  // Single group — render directly, no tabs needed
  if (tabs.length <= 1) {
    const singleTab = tabs[0];
    const items = singleTab ? groups[singleTab.key as Exclude<GroupKey, 'full'>] : groups.main;
    return (
      <div>
        {onRefresh && <RefreshIcon onClick={onRefresh} spinning={refreshing} />}
        {items.map((s, i) => (
          <div key={i} className="mb-2">
            {s.heading && <SectionHeading text={s.heading} />}
            <PrMarkdown text={s.content} />
          </div>
        ))}
      </div>
    );
  }

  // Multiple groups — add Full tab at the end
  tabs.push({ key: 'full', label: 'Full' });

  return (
    <TabGroup
      tabs={tabs}
      groups={groups}
      fullText={cleaned}
      onRefresh={onRefresh}
      refreshing={refreshing}
    />
  );
}

function TabGroup({
  tabs,
  groups,
  fullText,
  onRefresh,
  refreshing,
}: {
  tabs: Tab[];
  groups: Record<Exclude<GroupKey, 'full'>, Section[]>;
  fullText: string;
  onRefresh?: () => void;
  refreshing?: boolean;
}): React.ReactElement {
  const [active, setActive] = useState<GroupKey>(tabs[0].key);

  return (
    <div>
      {/* Sub-tab pills + refresh — same row, sticky */}
      <div className="sticky top-0 z-10 flex items-center gap-1 pb-3 bg-bg">
        {tabs.map((tab) => {
          const isActive = tab.key === active;
          return (
            <button
              key={tab.key}
              onClick={() => setActive(tab.key)}
              className="rounded-md px-2.5 py-1 text-label font-medium transition-colors"
              style={{
                color: isActive ? 'var(--color-text-1)' : 'var(--color-text-3)',
                background: isActive ? 'var(--color-surface-3)' : 'transparent',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              {tab.label}
            </button>
          );
        })}
        <span className="flex-1" />
        {onRefresh && <RefreshIcon onClick={onRefresh} spinning={refreshing} />}
      </div>

      {/* Active tab content */}
      <div>
        {active === 'full' ? (
          <PrMarkdown text={fullText} />
        ) : (
          (groups[active] ?? []).map((s, i) => (
            <div key={`${active}-${i}`} className="mb-2">
              {s.heading && <SectionHeading text={s.heading} />}
              <PrMarkdown text={s.content} />
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function RefreshIcon({
  onClick,
  spinning,
}: {
  onClick: () => void;
  spinning?: boolean;
}): React.ReactElement {
  return (
    <button
      onClick={onClick}
      disabled={spinning}
      className="flex items-center justify-center rounded-md transition-colors hover:bg-surface-2"
      style={{
        width: 24,
        height: 24,
        color: 'var(--color-text-3)',
        background: 'none',
        border: 'none',
        cursor: spinning ? 'default' : 'pointer',
        opacity: spinning ? 0.5 : 1,
      }}
      title="Refresh PR content"
    >
      <RefreshCw size={14} className={spinning ? 'animate-spin' : ''} />
    </button>
  );
}

function SectionHeading({ text }: { text: string }): React.ReactElement {
  return <div className="mt-3 mb-1 text-[12px] font-semibold text-text-2">{text}</div>;
}
