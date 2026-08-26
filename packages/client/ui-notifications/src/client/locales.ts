/** `notifications` namespace dictionaries. */

/** Dictionary namespace owned by this plugin. */
export const NS = 'notifications'

/** Simplified Chinese dictionary (the key-set source of truth). */
export const zh = {
  'trigger.label': '通知',
  'panel.aria': '通知中心',
  'notices.title': '系统通知',
  'notices.empty': '暂无通知',
  'notice.dismiss': '忽略此通知',
  'time.justnow': '刚刚',
  'time.minutes': '{minutes} 分钟前',
  'time.hours': '{hours} 小时前',
  'time.days': '{days} 天前',
  'sdk.title': 'AI SDK',
  'sdk.installed.label': '已安装',
  'sdk.installed.none': '未安装',
  'sdk.latest.label': '最新版本',
  'sdk.latest.none': '—',
  'sdk.install': '安装 {tag}',
  'sdk.installing': '正在安装…',
  'sdk.skip': '跳过此版本',
  'sdk.check': '检查更新',
  'sdk.checking': '正在检查…',
  'sdk.releaseNotes': '发布说明',
  'sdk.installedPendingRestart': '已安装 {tag}，下次启动生效',
} as const

/** English dictionary, key-identical to the Chinese source of truth. */
export const en: Record<NotificationsKey, string> = {
  'trigger.label': 'Notifications',
  'panel.aria': 'Notification center',
  'notices.title': 'Notices',
  'notices.empty': 'No notices',
  'notice.dismiss': 'Dismiss this notice',
  'time.justnow': 'just now',
  'time.minutes': '{minutes} min ago',
  'time.hours': '{hours} h ago',
  'time.days': '{days} d ago',
  'sdk.title': 'AI SDK',
  'sdk.installed.label': 'Installed',
  'sdk.installed.none': 'not installed',
  'sdk.latest.label': 'Latest',
  'sdk.latest.none': '—',
  'sdk.install': 'Install {tag}',
  'sdk.installing': 'Installing…',
  'sdk.skip': 'Skip this version',
  'sdk.check': 'Check now',
  'sdk.checking': 'Checking…',
  'sdk.releaseNotes': 'Release notes',
  'sdk.installedPendingRestart': 'Installed {tag} — takes effect next start',
}

/** Key domain of the `notifications` namespace (zh is the source of truth). */
export type NotificationsKey = keyof typeof zh
