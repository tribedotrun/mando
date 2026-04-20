import React from 'react';
import { RefreshCw } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import { TabsList, TabsTrigger } from '#renderer/global/ui/tabs';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/global/ui/tooltip';

interface TaskDetailTabBarProps {
  tabs: { key: string; label: string }[];
  effectiveTab: string;
  prNumber?: number | null;
  prRefreshing: boolean;
  onPrRefresh: () => void | Promise<void>;
}

export function TaskDetailTabBar({
  tabs,
  effectiveTab,
  prNumber,
  prRefreshing,
  onPrRefresh,
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
      {effectiveTab === 'pr' && prNumber && (
        <TooltipProvider delayDuration={300}>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                disabled={prRefreshing}
                onClick={() => void onPrRefresh()}
                className="mr-2 text-text-3 hover:text-text-1"
              >
                <RefreshCw size={14} className={prRefreshing ? 'animate-spin' : ''} />
                <span className="sr-only">Refresh PR</span>
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom" className="text-xs">
              Refresh PR
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}
    </div>
  );
}
