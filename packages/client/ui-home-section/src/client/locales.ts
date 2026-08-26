/** `home` namespace dictionaries: the sidebar Home section's inbox copy. */

/** Simplified Chinese dictionary (the key-set source of truth). */
export const zh = {
  'home.title': 'Home',
  'home.newSession': 'New Session',
  'home.newSession.failed': 'New session failed: {message}',
  'home.summary.running': '{n} running',
  'home.summary.sessions': '{n} sessions',
  'home.summary.workspaces': '{n} workspaces',
  'home.inbox.title': 'Recent',
  'home.inbox.empty': 'No sessions yet. Start one and it lands here.',
  'home.row.running': 'Running',
  'home.row.pending': 'Needs you',
  'home.row.done': 'Done',
  'time.now': 'now',
  'time.minutes': '{n}min',
  'time.hours': '{n}h',
  'time.days': '{n}d',
  'time.months': '{n}mo',
  'time.years': '{n}y',
} satisfies Record<string, string>

/** The home namespace key union. */
export type HomeKey = keyof typeof zh

/** English dictionary, checked complete against the zh key set. */
export const en = {
  'home.title': 'Home',
  'home.newSession': 'New Session',
  'home.newSession.failed': 'New session failed: {message}',
  'home.summary.running': '{n} running',
  'home.summary.sessions': '{n} sessions',
  'home.summary.workspaces': '{n} workspaces',
  'home.inbox.title': 'Recent',
  'home.inbox.empty': 'No sessions yet. Start one and it lands here.',
  'home.row.running': 'Running',
  'home.row.pending': 'Needs you',
  'home.row.done': 'Done',
  'time.now': 'now',
  'time.minutes': '{n}min',
  'time.hours': '{n}h',
  'time.days': '{n}d',
  'time.months': '{n}mo',
  'time.years': '{n}y',
} satisfies Record<HomeKey, string>
