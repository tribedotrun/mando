type DetailTab = 'feed' | 'pr' | 'deck' | 'more';

interface DetailTabItem {
  key: DetailTab;
  label: string;
}

export function buildTaskDetailTabs(hasDeck: boolean): DetailTabItem[] {
  return [
    { key: 'feed', label: 'Feed' },
    { key: 'pr', label: 'PR' },
    ...(hasDeck ? [{ key: 'deck' as const, label: 'Deck' }] : []),
    { key: 'more', label: 'More' },
  ];
}

export function resolveTaskDetailTab(
  requested: string | undefined,
  tabs: DetailTabItem[],
): DetailTab {
  return tabs.some((tab) => tab.key === requested) ? (requested as DetailTab) : 'feed';
}
