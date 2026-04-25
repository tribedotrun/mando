import React from 'react';
import { cn } from '#renderer/global/service/cn';
import { useAppHeader } from '#renderer/domains/captain/shell';
import { AppHeaderNav } from '#renderer/app/AppHeaderNav';
import { TaskTitleRow, HeaderMetaRow } from '#renderer/app/AppHeaderTask';

interface AppHeaderProps {
  sidebarCollapsed?: boolean;
  onToggleSidebar?: () => void;
  onGoBack?: () => void;
  onGoForward?: () => void;
  onNewTask?: () => void;
}

export function AppHeader({
  sidebarCollapsed,
  onToggleSidebar,
  onGoBack,
  onGoForward,
  onNewTask,
}: AppHeaderProps): React.ReactElement {
  const { ctx, isTerminalTab, pageTitle, sessions, resumeMut, taskIsRateLimited } = useAppHeader();

  const navIcons = sidebarCollapsed ? (
    <AppHeaderNav
      onToggleSidebar={onToggleSidebar}
      onGoBack={onGoBack}
      onGoForward={onGoForward}
      onNewTask={onNewTask}
    />
  ) : null;

  if (!ctx) {
    if (sidebarCollapsed) {
      return (
        <div
          className="relative z-10 flex h-[38px] shrink-0 items-start bg-background pl-[70px] pt-[10px]"
          style={{ WebkitAppRegion: 'drag' }}
        >
          {navIcons}
          <span className="ml-3 min-w-0 truncate text-body font-medium text-foreground">
            {pageTitle}
          </span>
        </div>
      );
    }
    return (
      <div
        className="relative z-10 h-10 shrink-0 bg-background"
        style={{ WebkitAppRegion: 'drag' }}
      />
    );
  }

  const hasTask = !!ctx.task;

  return (
    <div
      className={cn(
        'flex shrink-0 flex-col justify-center border-b border-border',
        sidebarCollapsed ? 'pb-2 pr-6 pt-[10px]' : 'px-6 py-2',
      )}
      style={{ WebkitAppRegion: 'drag' }}
    >
      {hasTask ? (
        <TaskTitleRow
          ctx={ctx}
          sidebarCollapsed={sidebarCollapsed}
          navIcons={navIcons}
          isTerminalTab={isTerminalTab}
          taskIsRateLimited={taskIsRateLimited}
          resumeMut={resumeMut}
        />
      ) : (
        sidebarCollapsed && (
          <div className="flex items-center pl-[70px]">
            {navIcons}
            {ctx.worktreeName && (
              <span
                className="ml-3 min-w-0 truncate text-body font-medium text-foreground"
                title={ctx.worktreeName}
              >
                {ctx.worktreeName}
              </span>
            )}
          </div>
        )
      )}
      {/* Row 2: status + project + worktree */}
      <HeaderMetaRow
        ctx={ctx}
        hasTask={hasTask}
        sidebarCollapsed={sidebarCollapsed}
        sessions={sessions}
      />
    </div>
  );
}
