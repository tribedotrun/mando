export type UpdateChannel = 'stable' | 'beta';

export interface FeedResponse {
  url: string;
  name: string;
  notes: string;
  pub_date: string;
}

export interface PendingUpdate {
  version: string;
  notes: string;
  appPath: string;
}

export type FeedResult =
  | { kind: 'update'; feed: FeedResponse }
  | { kind: 'up-to-date' }
  | { kind: 'error' };
