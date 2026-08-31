import React from 'react';
import {
  Bot,
  ChevronDown,
  ChevronRight,
  FilePenLine,
  Globe2,
  Image,
  Search,
  Terminal,
  Wrench,
} from 'lucide-react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '#renderer/global/ui/primitives/collapsible';
import {
  selectToolOpenState,
  useTranscriptUi,
} from '#renderer/domains/sessions/runtime/useTranscriptUi';

interface ToolFrameProps {
  id: string;
  name: string;
  summary?: React.ReactNode;
  isError?: boolean;
  resultBadge?: React.ReactNode;
  children?: React.ReactNode;
  defaultOpen?: boolean;
}

function ToolGlyph({ name }: { name: string }): React.ReactElement {
  const key = name.toLowerCase();
  if (key === 'bash' || key === 'ran') return <Terminal size={14} />;
  if (key === 'files' || key === 'edit' || key === 'write') return <FilePenLine size={14} />;
  if (key === 'viewed') return <Image size={14} />;
  if (key === 'grep' || key === 'glob' || key === 'read') return <Search size={14} />;
  if (key === 'websearch' || key === 'webfetch' || key === 'mcp') return <Globe2 size={14} />;
  if (key === 'task' || key === 'explored') return <Bot size={14} />;
  return <Wrench size={14} />;
}

export function ToolFrame({
  id,
  name,
  summary,
  isError,
  resultBadge,
  children,
  defaultOpen = false,
}: ToolFrameProps): React.ReactElement {
  const userOverride = useTranscriptUi(selectToolOpenState(id));
  const setToolExpanded = useTranscriptUi((s) => s.setToolExpanded);
  const open = userOverride ?? defaultOpen;
  const hasBody = Boolean(children);

  return (
    <Collapsible open={open} onOpenChange={(v) => setToolExpanded(id, v)} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button
          className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-label text-muted-foreground${hasBody ? ' hover:bg-muted/70' : ''}`}
          style={{ cursor: hasBody ? 'pointer' : 'default' }}
          onClick={(e) => {
            if (!hasBody) e.preventDefault();
          }}
        >
          <span className="shrink-0 text-text-3">
            <ToolGlyph name={name} />
          </span>
          <span className={`font-medium ${isError ? 'text-destructive' : 'text-foreground'}`}>
            {name}
          </span>
          {summary && (
            <span className="min-w-0 truncate text-label normal-case opacity-60">{summary}</span>
          )}
          <span className="ml-auto flex items-center gap-1">
            {resultBadge}
            {hasBody && (open ? <ChevronDown size={11} /> : <ChevronRight size={11} />)}
          </span>
        </button>
      </CollapsibleTrigger>
      {hasBody && (
        <CollapsibleContent>
          <div className="pb-2 pl-8">{children}</div>
        </CollapsibleContent>
      )}
    </Collapsible>
  );
}
