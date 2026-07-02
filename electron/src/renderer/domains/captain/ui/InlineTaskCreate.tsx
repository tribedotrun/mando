import React, { useImperativeHandle } from 'react';
import { ListChecks } from 'lucide-react';
import { useInlineTaskCreate } from '#renderer/domains/captain/runtime/useInlineTaskCreate';
import { Button } from '#renderer/global/ui/primitives/button';
import {
  TaskCreateOptionsMenu,
  TaskProjectSelect,
  TaskSubmitButton,
} from '#renderer/domains/captain/ui/TaskComposerControls';

export interface InlineTaskCreateHandle {
  focus: () => void;
}

interface InlineTaskCreateProps {
  ref?: React.Ref<InlineTaskCreateHandle>;
}

export function InlineTaskCreate({ ref }: InlineTaskCreateProps): React.ReactElement {
  const form = useInlineTaskCreate();
  const { bulk, setBulk } = form.draft;

  useImperativeHandle(ref, () => ({
    focus: () => form.draft.inputRef.current?.focus(),
  }));

  return (
    <div className="mx-auto w-full max-w-[640px]">
      <div className="rounded-xl bg-muted">
        <textarea
          ref={form.draft.inputRef}
          data-testid="inline-task-input"
          value={form.draft.title}
          onChange={(event) => form.draft.setTitle(event.target.value)}
          onKeyDown={form.events.handleKeyDown}
          onPaste={form.events.handlePaste}
          placeholder={
            bulk
              ? 'Describe your tasks, one per line, or free-form.\nAI will parse individual items.'
              : 'What needs to be done?'
          }
          rows={form.draft.textareaRows}
          className="w-full resize-none rounded-xl bg-transparent px-4 pb-2 pt-4 text-sm text-foreground placeholder:text-text-3 focus:outline-none"
          style={{ caretColor: 'var(--foreground)' }}
        />

        {!bulk && form.image.preview && form.image.image && (
          <div className="flex items-center gap-3 px-4 pb-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-secondary">
              <img
                src={form.image.preview}
                alt={form.image.image.name}
                className="max-h-10 max-w-10 rounded-md object-contain"
              />
            </div>
            <span className="min-w-0 truncate text-caption text-muted-foreground">
              {form.image.image.name}
            </span>
            <Button
              variant="ghost"
              size="xs"
              onClick={form.image.removeImage}
              className="text-text-3"
            >
              Remove
            </Button>
          </div>
        )}

        <div className="flex items-center gap-2 px-3 pb-3">
          <TaskProjectSelect
            projects={form.project.projects}
            value={form.project.effectiveProject}
            onValueChange={form.project.handleProjectChange}
            testId="inline-task-project"
          />
          {!bulk && (
            <TaskCreateOptionsMenu
              provider={form.routing.provider}
              onProviderChange={form.routing.setProvider}
              planning={form.routing.planning}
              onPlanningChange={form.routing.setPlanning}
              useGlmWorker={form.routing.useGlmWorker}
              defaultGlmWorker={form.routing.defaultGlmWorker}
              onUseGlmWorkerChange={form.routing.setUseGlmWorker}
              globalAutoMerge={form.autoMerge.globalAutoMerge}
              noAutoMerge={form.autoMerge.noAutoMerge}
              onNoAutoMergeChange={form.autoMerge.setNoAutoMerge}
              onImageSelect={form.image.setImageFile}
            />
          )}
          <Button
            variant={bulk ? 'outline' : 'ghost'}
            size="icon-sm"
            onClick={() => setBulk(!bulk)}
            className={bulk ? 'bg-accent text-foreground' : 'text-text-3'}
            data-testid="inline-task-bulk-toggle"
            aria-label={bulk ? 'Disable bulk task mode' : 'Enable bulk task mode'}
            aria-pressed={bulk}
            title={bulk ? 'Bulk mode on' : 'Bulk mode'}
          >
            <ListChecks size={16} />
          </Button>

          <span className="flex-1" />

          {form.project.projectRequired && !form.project.effectiveProject && (
            <span className="text-caption text-text-3">Choose a project</span>
          )}

          <TaskSubmitButton
            testId="inline-task-submit"
            disabled={!form.submit.canSubmit}
            pending={form.submit.pending}
            onSubmit={() => void form.submit.handleSubmit()}
          />
        </div>
      </div>
    </div>
  );
}
