/** `team` namespace dictionaries: the sidebar Team section's copy. */

/** Simplified Chinese dictionary (the key-set source of truth). */
export const zh = {
  'team.members.title': 'Members',
  'team.members.empty': 'No agents are running. Delegate work to a subagent and it appears here.',
  'team.member.running': 'Running',
  'team.member.inactive': 'Inactive',
  'team.roster.title': 'Teammates',
  'team.roster.empty': 'No agent presets are composed for this deployment.',
  'team.roster.error': 'Roster unavailable: {message}',
  'team.roster.broken': 'Unavailable: {reason}',
  'team.start': 'Start session',
  'team.start.failed': 'Could not start a session: {message}',
  'team.trust.system': 'System',
  'team.trust.user': 'User',
  'time.now': 'now',
  'time.minutes': '{n}min',
  'time.hours': '{n}h',
  'time.days': '{n}d',
  'time.months': '{n}mo',
  'time.years': '{n}y',
} satisfies Record<string, string>

/** The team namespace key union. */
export type TeamKey = keyof typeof zh

/** English dictionary, checked complete against the zh key set. */
export const en = {
  'team.members.title': 'Members',
  'team.members.empty': 'No agents are running. Delegate work to a subagent and it appears here.',
  'team.member.running': 'Running',
  'team.member.inactive': 'Inactive',
  'team.roster.title': 'Teammates',
  'team.roster.empty': 'No agent presets are composed for this deployment.',
  'team.roster.error': 'Roster unavailable: {message}',
  'team.roster.broken': 'Unavailable: {reason}',
  'team.start': 'Start session',
  'team.start.failed': 'Could not start a session: {message}',
  'team.trust.system': 'System',
  'team.trust.user': 'User',
  'time.now': 'now',
  'time.minutes': '{n}min',
  'time.hours': '{n}h',
  'time.days': '{n}d',
  'time.months': '{n}mo',
  'time.years': '{n}y',
} satisfies Record<TeamKey, string>
