import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { localizeMeta } from '#renderer/global/service/utils';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import { Button } from '#renderer/global/ui/button';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/global/ui/collapsible';
import {
  type TranscriptSection,
  type ContentBlock,
  textBlocksOf,
  toolBlocksOf,
} from '#renderer/domains/sessions/service/transcript';
import { detectSkill } from '#renderer/domains/sessions/service/helpers';
import { ToolBlock, ErrorBlock } from '#renderer/domains/sessions/ui/TranscriptToolBlocks';

export function HumanSection({ section }: { section: TranscriptSection }): React.ReactElement {
  const textBlocks = textBlocksOf(section);
  const isCommand = section.heading.startsWith('/');

  const totalContent = textBlocks.map((b) => b.content).join('\n');
  const skillName = detectSkill(totalContent);
  const [open, setOpen] = useState(false);

  if (skillName) {
    return (
      <div className="mt-4 rounded-lg bg-muted/40 px-3.5 py-3">
        <div className="flex items-center gap-2 text-label">
          <span className="font-medium text-foreground">{section.heading}</span>
          {section.meta && (
            <span className="font-mono normal-case text-muted-foreground">
              {localizeMeta(section.meta)}
            </span>
          )}
        </div>
        <Collapsible open={open} onOpenChange={setOpen}>
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="mt-1.5 flex items-center gap-1.5 rounded px-1 py-0.5 text-label hover:bg-muted"
            >
              {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span className="font-mono font-medium text-muted-foreground">/{skillName}</span>
            </button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            {textBlocks.map((block, bi) => (
              <div key={bi} className="text-caption leading-relaxed text-foreground">
                <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
              </div>
            ))}
          </CollapsibleContent>
        </Collapsible>
      </div>
    );
  }

  return (
    <div className="mt-4 rounded-lg bg-muted/40 px-3.5 py-3">
      <div className="flex items-center gap-2 pb-1.5 text-label">
        <span
          className={
            isCommand
              ? 'font-mono font-medium normal-case text-muted-foreground'
              : 'font-medium text-foreground'
          }
        >
          {section.heading}
        </span>
        {section.meta && (
          <span className="font-mono normal-case text-muted-foreground">
            {localizeMeta(section.meta)}
          </span>
        )}
      </div>
      {textBlocks.map((block, bi) => (
        <div key={bi} className="text-caption leading-relaxed text-foreground">
          <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
        </div>
      ))}
    </div>
  );
}

export function TurnContent({ section }: { section: TranscriptSection }): React.ReactElement {
  const [showTools, setShowTools] = useState(false);
  const textBlocks = textBlocksOf(section);
  const toolBlocks = toolBlocksOf(section);
  const toolCount = toolBlocks.filter((b) => b.type === 'tool').length;

  return (
    <>
      {toolBlocks.length > 0 && (
        <Collapsible open={showTools} onOpenChange={setShowTools}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="xs" className="mt-1 gap-1.5 text-muted-foreground">
              {showTools ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span>
                {toolCount} tool call{toolCount !== 1 ? 's' : ''}
              </span>
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            {toolBlocks.map((block, bi) => {
              if (block.type === 'tool') return <ToolBlock key={bi} block={block} />;
              if (block.type === 'error')
                return (
                  <ErrorBlock key={bi} block={block as Extract<ContentBlock, { type: 'error' }>} />
                );
              if (block.type === 'results') {
                return (
                  <div key={bi} className="py-1 text-label italic text-muted-foreground">
                    {block.content}
                  </div>
                );
              }
              return null;
            })}
          </CollapsibleContent>
        </Collapsible>
      )}
      {textBlocks.map((block, bi) => (
        <div key={`t-${bi}`} className="py-1 text-caption leading-relaxed text-foreground">
          <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
        </div>
      ))}
    </>
  );
}

export function SectionRow({ section }: { section: TranscriptSection }): React.ReactElement {
  if (section.kind === 'human') return <HumanSection section={section} />;
  return (
    <div>
      <div className="flex items-center gap-2 pb-1 pt-3 text-label text-muted-foreground">
        <span className="font-medium">{section.heading}</span>
        {section.meta && (
          <span className="font-mono normal-case">{localizeMeta(section.meta)}</span>
        )}
      </div>
      <TurnContent section={section} />
    </div>
  );
}
