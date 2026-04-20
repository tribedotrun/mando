import React from 'react';
import { useTaskDetailView } from '#renderer/domains/captain/runtime/useTaskDetailView';
import { useTaskAsk } from '#renderer/domains/captain/runtime/useTaskAsk';
import type { TaskItem } from '#renderer/global/types';
import { TaskActionBar } from '#renderer/domains/captain/ui/TaskActionBar';
import { PrTab, InfoTab, ContextModal } from '#renderer/domains/captain/ui/TaskDetailTabs';
import { SessionsTab } from '#renderer/domains/captain/ui/SessionsTab';
import { TaskFeedView } from '#renderer/domains/captain/ui/TaskFeedView';
import { TaskDetailTabBar } from '#renderer/domains/captain/ui/TaskDetailViewParts';
import { cn } from '#renderer/global/service/cn';
import { Tabs } from '#renderer/global/ui/tabs';

interface Props {
  item: TaskItem;
  onBack: () => void;
  onOpenTranscript?: (opts: {
    sessionId: string;
    caller?: string;
    cwd?: string;
    project?: string;
    taskTitle?: string;
  }) => void;
  activeTab?: string;
  onTabChange?: (tab: string) => void;
  onResumeInTerminal?: (sessionId: string, name?: string) => void;
  terminalSlot?: React.ReactNode;
}

export function TaskDetailView({
  item,
  onBack,
  onOpenTranscript,
  activeTab: activeTabProp,
  onTabChange,
  onResumeInTerminal,
  terminalSlot,
}: Props): React.ReactElement {
  const { ask } = useTaskAsk(item.id);
  const {
    tabs,
    effectiveTab,
    prRefreshing,
    contextModalOpen,
    setContextModalOpen,
    prBody,
    prPending,
    sessions,
    handleSessionClick,
    handleResumeSession,
    handlePrRefresh,
  } = useTaskDetailView({ item, onBack, onOpenTranscript, onResumeInTerminal, activeTabProp });

  return (
    <div className="flex h-full flex-col">
      {/* Main row */}
      <div className="flex min-h-0 flex-1">
        {/* Left column, entire column scrolls together */}
        <div
          className={cn(
            'min-h-0 min-w-0 flex-1 overflow-x-hidden',
            effectiveTab === 'feed' || effectiveTab === 'terminal'
              ? 'flex flex-col overflow-hidden'
              : 'scrollbar-on-hover overflow-y-auto',
          )}
        >
          <Tabs
            value={effectiveTab}
            onValueChange={(v) => onTabChange?.(v)}
            className={cn(
              'gap-0',
              (effectiveTab === 'feed' || effectiveTab === 'terminal') &&
                'flex flex-1 flex-col min-h-0',
            )}
          >
            <TaskDetailTabBar
              tabs={tabs}
              effectiveTab={effectiveTab}
              prNumber={item.pr_number}
              prRefreshing={prRefreshing}
              onPrRefresh={handlePrRefresh}
            />

            {/* Tab content */}
            {effectiveTab !== 'terminal' && (
              <div
                className={cn(
                  'break-words',
                  effectiveTab === 'feed' ? 'flex-1 min-h-0' : 'px-3 pt-3',
                )}
              >
                {effectiveTab === 'feed' && <TaskFeedView key={item.id} item={item} />}
                {effectiveTab === 'pr' && (
                  <PrTab item={item} prBody={prBody} prPending={prPending} />
                )}
                {effectiveTab === 'more' && (
                  <div className="space-y-6">
                    <InfoTab item={item} />
                    <SessionsTab
                      sessions={sessions}
                      onSessionClick={handleSessionClick}
                      onResumeSession={handleResumeSession}
                      taskId={item.id}
                    />
                  </div>
                )}
              </div>
            )}
            {/* Terminal stays mounted (display:none) to keep xterm alive */}
            {terminalSlot && (
              <div className={cn(effectiveTab === 'terminal' ? 'flex-1 min-h-0' : 'hidden')}>
                {terminalSlot}
              </div>
            )}
          </Tabs>
        </div>
      </div>

      {/* Action bar: only on PR and More tabs (feed has its own input, terminal doesn't need one) */}
      {effectiveTab !== 'feed' && effectiveTab !== 'terminal' && (
        <TaskActionBar item={item} onAsk={(q, images) => void ask(q, images)} />
      )}

      {/* Context modal */}
      {contextModalOpen && item.context && (
        <ContextModal context={item.context} onClose={() => setContextModalOpen(false)} />
      )}
    </div>
  );
}
