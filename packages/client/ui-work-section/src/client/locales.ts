/** `work` namespace dictionaries: the sidebar Work section's copy. */

/** Simplified Chinese dictionary (the key-set source of truth). */
export const zh = {
  'work.attention.title': 'Needs you',
  'work.attention.empty': 'Nothing is waiting on you.',
  'work.running.title': 'Running',
  'work.running.empty': 'No sessions are running right now.',
  'work.goals.title': 'Goals',
  'work.goals.empty': 'Open a session to see its goal here.',
  'work.goal.blocked': 'Blocked',
  'work.goal.paused': 'Paused',
  'work.goal.active': 'Active',
  'work.row.pending': 'Needs you',
  'work.row.running': 'Running',
  'work.row.done': 'Done',
  'time.now': 'now',
  'time.minutes': '{n}min',
  'time.hours': '{n}h',
  'time.days': '{n}d',
  'time.months': '{n}mo',
  'time.years': '{n}y',
} satisfies Record<string, string>

/** The work namespace key union. */
export type WorkKey = keyof typeof zh

/** English dictionary, checked complete against the zh key set. */
export const en = {
  'work.attention.title': 'Needs you',
  'work.attention.empty': 'Nothing is waiting on you.',
  'work.running.title': 'Running',
  'work.running.empty': 'No sessions are running right now.',
  'work.goals.title': 'Goals',
  'work.goals.empty': 'Open a session to see its goal here.',
  'work.goal.blocked': 'Blocked',
  'work.goal.paused': 'Paused',
  'work.goal.active': 'Active',
  'work.row.pending': 'Needs you',
  'work.row.running': 'Running',
  'work.row.done': 'Done',
  'time.now': 'now',
  'time.minutes': '{n}min',
  'time.hours': '{n}h',
  'time.days': '{n}d',
  'time.months': '{n}mo',
  'time.years': '{n}y',
} satisfies Record<WorkKey, string>
