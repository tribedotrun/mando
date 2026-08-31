import React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import type { UserEvent } from '#renderer/global/types';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '#renderer/global/ui/primitives/collapsible';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import {
  extractSkillName,
  isSkillPromptBody,
  mandoPromptLabel,
} from '#renderer/domains/sessions/service/transcriptEvents';
import {
  selectToolOpenState,
  useTranscriptUi,
} from '#renderer/domains/sessions/runtime/useTranscriptUi';

interface UserMessageProps {
  event: UserEvent;
  eventIndex: number;
}

export function UserMessage({ event, eventIndex }: UserMessageProps): React.ReactElement | null {
  const promptId = `${eventIndex}-injected-prompt`;
  const userOverride = useTranscriptUi(selectToolOpenState(promptId));
  const setToolExpanded = useTranscriptUi((s) => s.setToolExpanded);

  const texts: string[] = [];
  let imageCount = 0;
  for (const block of event.blocks) {
    if (block.kind === 'text') {
      const t = block.data.text.trim();
      if (t) texts.push(t);
    } else if (block.kind === 'image') {
      imageCount++;
    }
  }
  if (texts.length === 0 && imageCount === 0) return null;
  const body = texts.join('\n').trim();
  if (body.includes('<local-command-caveat>') || body.includes('<local-command-stdout>')) {
    return null;
  }

  const skillPrompt = isSkillPromptBody(body);
  const mandoLabel = mandoPromptLabel(body);
  if (skillPrompt || mandoLabel) {
    const open = userOverride ?? false;
    const detail = skillPrompt ? extractSkillName(body) : null;
    const label = skillPrompt ? 'Skill instructions' : mandoLabel;
    return (
      <Collapsible
        open={open}
        onOpenChange={(v) => setToolExpanded(promptId, v)}
        className="my-0.5"
      >
        <CollapsibleTrigger asChild>
          <button className="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-label text-muted-foreground hover:bg-muted">
            <span className="font-medium text-foreground">{label}</span>
            {detail && <span className="opacity-60">{detail}</span>}
            <span className="ml-auto">
              {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
            </span>
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="mt-1 border-l-2 border-accent/60 bg-muted/30 py-2 pl-3 pr-2">
            <div className="text-sm text-foreground">
              <PrMarkdown text={body} />
            </div>
            {imageCount > 0 && (
              <p className="mt-1 text-label text-muted-foreground">
                + {imageCount} image{imageCount > 1 ? 's' : ''} attached
              </p>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>
    );
  }

  return (
    <div className="border-l-2 border-accent/60 bg-muted/30 py-2 pl-3 pr-2">
      {body && (
        <div className="text-sm text-foreground">
          <PrMarkdown text={body} />
        </div>
      )}
      {imageCount > 0 && (
        <p className="mt-1 text-label text-muted-foreground">
          + {imageCount} image{imageCount > 1 ? 's' : ''} attached
        </p>
      )}
    </div>
  );
}
