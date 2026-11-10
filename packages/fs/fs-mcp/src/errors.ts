/**
 * Translation of nc-tools' error taxonomy into the filesystem seam's.
 *
 * nc-tools reports a structured `{code, message, hint}` for every failure —
 * 45 registered codes, each with a machine-readable meaning. The filesystem
 * seam has 13 {@link FsErrorCode}s. This module is the one place the two
 * vocabularies meet, so a consumer branching on `FsErrorCode` never has to
 * know that nc-tools exists.
 *
 * @module @deepseek-ai/dsh-fs-mcp/errors
 */

import { FsError } from '@deepseek-ai/dsh-fs'
import type { FsErrorCode } from '@deepseek-ai/dsh-fs'
import type { NcToolError } from './types.ts'

/**
 * nc-tools code → filesystem code.
 *
 * The mapping is by MEANING, not by name: nc-tools' `PATCH_AMBIGUOUS`
 * ("the old text matched more than once") is the seam's
 * `FS_AMBIGUOUS_EDIT`, and its `PATCH_NO_MATCH` is `FS_EDIT_NOT_FOUND`.
 * A code with no filesystem equivalent becomes `FS_IO_ERROR` — the
 * catch-all for "the operation failed for a reason the seam does not
 * model" — rather than a fabricated specific code.
 */
const errorCodes: Readonly<Record<string, FsErrorCode>> = {
  // The target is not there.
  ERR_NOT_FOUND: 'FS_NOT_FOUND',
  // The target is there but is not the kind the operation needs.
  ERR_IS_DIRECTORY: 'FS_NOT_DIRECTORY',
  ERR_BINARY_FILE: 'FS_NOT_TEXT',
  ERR_NOT_A_REPO: 'FS_NOT_REGULAR_FILE',
  // The edit did not match, or matched more than once.
  PATCH_NO_MATCH: 'FS_EDIT_NOT_FOUND',
  PATCH_AMBIGUOUS: 'FS_AMBIGUOUS_EDIT',
  ERR_BAD_EDIT: 'FS_EDIT_NOT_FOUND',
  // The file moved under the caller between the guard and the write.
  PATCH_STALE_READ: 'FS_STALE_VERSION',
  // The caller was refused.
  ERR_PERMISSION: 'FS_PERMISSION_DENIED',
  ERR_REFUSED: 'FS_PERMISSION_DENIED',
  ERR_SCOPE_DENIED: 'FS_SANDBOX_DENIED',
  // The result was too big to hand over whole.
  ERR_ARCHIVE_TOO_LARGE: 'FS_TOO_LARGE',
  ERR_TOO_LARGE: 'FS_TOO_LARGE',
  // The work was cancelled.
  ERR_CANCELLED: 'FS_ABORTED',
}

/**
 * Translate one nc-tools error into the seam's vocabulary.
 *
 * @param error - The structured error nc-tools reported.
 * @param fallback - The code to use when the nc-tools code has no equivalent.
 * @returns An {@link FsError} carrying the harness code and the nc-tools
 *   error as its cause.
 */
export function toFsError(error: NcToolError, fallback: FsErrorCode = 'FS_IO_ERROR'): FsError {
  const code = errorCodes[error.code] ?? fallback
  return new FsError(error.message, code, { cause: error })
}
