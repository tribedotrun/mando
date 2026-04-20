import { useMemo } from 'react';
import { useConfig } from '#renderer/global/repo/queries';
import type { SettingsSection } from '#renderer/domains/settings/types';

export interface NavItem {
  id: SettingsSection;
  label: string;
}

const BASE_NAV_ITEMS: readonly NavItem[] = Object.freeze([
  { id: 'general', label: 'General' },
  { id: 'projects', label: 'Projects' },
  { id: 'captain', label: 'Captain' },
  { id: 'credentials', label: 'Credentials' },
  { id: 'telegram', label: 'Telegram' },
  { id: 'experimental', label: 'Experimental' },
  { id: 'about', label: 'About' },
]);

export function useSettingsPage(sectionProp: SettingsSection) {
  const { data: config, isLoading, error } = useConfig();
  const scoutEnabled = !!config?.features?.scout;

  const navItems = useMemo(() => {
    let items = BASE_NAV_ITEMS;
    if (scoutEnabled) {
      const idx = items.findIndex((i) => i.id === 'experimental');
      items = [
        ...items.slice(0, idx),
        { id: 'scout' as SettingsSection, label: 'Scout' },
        ...items.slice(idx),
      ];
    }
    return items;
  }, [scoutEnabled]);

  const section: SettingsSection = navItems.some((item) => item.id === sectionProp)
    ? sectionProp
    : 'general';

  return { navItems, section, isLoading, error };
}
