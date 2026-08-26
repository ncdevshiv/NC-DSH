/**
 * Install-directory layout of the sidecar-update pipeline: asset naming,
 * release/download path derivation, and the pointer document name. Every
 * function here is a pure mapping; persistence lives in the persist module.
 *
 * Layout under the resolved install directory:
 *
 * ```text
 * current.json                    pointer: {tag, asset, sha256, installedAt, exePath}
 * ignored.json                    tags ignored through ignore()
 * downloads/<tag>/<asset>         staged download bytes
 * releases/<tag>/ai-sidecar[.exe] installed executables, never overwritten in place
 * ```
 * @module @deepseek-ai/dsh-sidecar-updates/layout
 */

import { join } from 'node:path'
import type { ReleaseTarget } from './types.ts'

/** Pointer document inside the install directory. */
export const POINTER_FILENAME = 'current.json'
/** Ignore-list document inside the install directory. */
export const IGNORED_FILENAME = 'ignored.json'
/** Checksum manifest asset expected on every installable release. */
export const SHA256SUMS_ASSET = 'SHA256SUMS'

/**
 * Product-fixed executable basename under `releases/<tag>/`; consumers launch
 * this path directly, independent of the release-asset naming.
 */
export const INSTALLED_EXE_BASENAME = 'ai-sidecar'

/**
 * Derive the release asset filename for one target. Windows assets carry an
 * `.exe` suffix; every other platform is bare.
 * @param prefix - configured asset prefix (`ai-sidecar` by default).
 * @param target - platform/arch pair selecting the asset.
 * @returns the expected asset filename.
 */
export function assetNameFor(prefix: string, target: ReleaseTarget): string {
  const suffix = target.platform === 'win32' ? '.exe' : ''
  return `${prefix}-${target.platform}-${target.arch}${suffix}`
}

/**
 * Whether the pipeline can serve this target at all.
 * @param target - platform/arch pair to judge.
 * @returns whether both the platform and architecture are supported.
 */
export function isSupportedTarget(target: ReleaseTarget): boolean {
  return (target.platform === 'darwin' || target.platform === 'linux' || target.platform === 'win32')
    && (target.arch === 'x64' || target.arch === 'arm64')
}

/**
 * Directory receiving the staged download of one release's assets.
 * @param installDir - resolved install directory root.
 * @param tag - release tag being downloaded.
 * @returns the absolute staging directory path.
 */
export function downloadDirFor(installDir: string, tag: string): string {
  return join(installDir, 'downloads', tag)
}

/**
 * Absolute executable path of one installed release. Each tag owns its own
 * directory, so installing never overwrites a running binary.
 * @param installDir - resolved install directory root.
 * @param target - platform/arch pair selecting the executable suffix.
 * @param tag - release tag being installed.
 * @returns the absolute executable path.
 */
export function exePathFor(installDir: string, target: ReleaseTarget, tag: string): string {
  const suffix = target.platform === 'win32' ? '.exe' : ''
  return join(installDir, 'releases', tag, `${INSTALLED_EXE_BASENAME}${suffix}`)
}
