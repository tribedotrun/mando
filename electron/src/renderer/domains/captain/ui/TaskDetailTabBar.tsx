import React from 'react';
import { CircleStop, RefreshCw } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { TabsList, TabsTrigger } from '#renderer/global/ui/primitives/tabs';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/global/ui/primitives/tooltip';

interface TaskDetailTabBarProps {
  tabs: { key: string; label: string }[];
  effectiveTab: string;
  prNumber?: number | null;
  prRefreshing: boolean;
  onPrRefresh: () => void | Promise<void>;
  canStop?: boolean;
  stopPending?: boolean;
  onStop?: () => void;
}

export function TaskDetailTabBar({
  tabs,
  effectiveTab,
  prNumber,
  prRefreshing,
  onPrRefresh,
  canStop,
  stopPending,
  onStop,
}: TaskDetailTabBarProps): React.ReactElement {
  return (
    <div className="sticky top-0 z-10 flex items-center justify-between bg-background">
      <TabsList variant="line" className="h-auto gap-0">
        {tabs.map((tab) => (
          <TabsTrigger key={tab.key} value={tab.key} className="px-3 py-1.5 text-body font-medium">
            {tab.label}
          </TabsTrigger>
        ))}
      </TabsList>
      <TooltipProvider delayDuration={300}>
        <div className="flex items-center gap-1 pr-2">
          {canStop && onStop && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  disabled={stopPending}
                  onClick={onStop}
                  data-testid="task-detail-stop-btn"
                  className="text-destructive hover:bg-destructive/10"
                >
                  <CircleStop size={14} />
                  <span className="sr-only">Stop worker</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="text-xs">
                Stop worker (worktree preserved)
              </TooltipContent>
            </Tooltip>
          )}
          {effectiveTab === 'pr' && prNumber && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  disabled={prRefreshing}
                  onClick={() => void onPrRefresh()}
                  className="text-text-3 hover:text-text-1"
                >
                  <RefreshCw size={14} className={prRefreshing ? 'animate-spin' : ''} />
                  <span className="sr-only">Refresh PR</span>
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="text-xs">
                Refresh PR
              </TooltipContent>
            </Tooltip>
          )}
        </div>
      </TooltipProvider>
    </div>
  );
}
