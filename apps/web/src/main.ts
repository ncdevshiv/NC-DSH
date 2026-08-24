/**
 * Web application entry: thin bootstrap over the shell library. Everything —
 * module-table seeding, the boot page, and the UI-renderer handoff — lives
 * in @deepseek-ai/dsh-client-web; this file only finds the mount point and
 * marks the desktop shell for the caption-band drag rules (base.css).
 */
import { AppWebEntry } from '@deepseek-ai/dsh-client-web'

const el = document.getElementById('root')
if (el === null) throw new Error('web app: missing #root')
// Desktop-shell marker for base.css: the caption-band drag rules (window move
// support for the Electron shell's hidden title bar) key on this attribute so
// ordinary browsers never reserve title-bar space. Same feature-detect as
// ThemePresenter's chrome sync and the boot theme script.
if ((window as unknown as { dshDesktop?: object }).dshDesktop !== undefined) {
  document.documentElement.setAttribute('data-dsh-desktop', '')
}
void new AppWebEntry(el).run()
