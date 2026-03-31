import contract from '#contracts/capabilities.json';

type Domain = keyof typeof contract;

export function hasCapability(domain: Domain, key: string): boolean {
  return key in contract[domain];
}

export const SCOUT_PROCESS_LABEL = hasCapability('scout', 'process')
  ? 'Process Scout items'
  : 'Process';

export const CAPTAIN_TRIAGE_LABEL = hasCapability('captain', 'triage')
  ? 'Triage reviews'
  : 'Triage';
