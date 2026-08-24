//! landlock-run: self-restrict-then-exec Landlock launcher.
//!
//! The Landlock rung of a consuming sandbox seam, for Linux hosts where
//! `bwrap` is unusable (not installed, unprivileged user namespaces disabled,
//! or an LSM profile that denies mount — Landlock is an independent syscall
//! family and needs none of those). The launcher installs a Landlock ruleset
//! on itself and `exec`s the wrapped command; the ruleset is inherited across
//! `execve`, so the command (and every process it spawns) runs confined while
//! the invoking process stays unrestricted.
//!
//! CLI contract (mirrors the `bwrap` runner argv shape the executor wraps):
//!
//! ```text
//! landlock-run [--ro <path>]... [--rw <path>]... -- <argv>...
//! landlock-run --probe
//! ```
//!
//! `--ro` grants read+execute beneath the path; `--rw` grants full filesystem
//! access beneath the path. Everything else is denied (Landlock is an
//! allow-list). `--probe` builds a maximal ruleset and reports whether the
//! running kernel actually enforces it — the executor's functional probe.
//!
//! Fail-closed: if the ruleset cannot be created or is NOT enforced by the
//! kernel, the launcher exits non-zero WITHOUT exec'ing the command. A
//! partial (best-effort) enforcement on an older ABI is accepted and reported
//! on stderr; the consumer's mode vocabulary keeps its file-effect promises
//! honest per ABI level (surfaced as `full` vs `partial` by the entry
//! package's probe).
//!
//! Single-file Rust over the raw Landlock UAPI. The only dependency is the
//! `libc` crate's syscall shims (statically linked against musl), so the whole
//! audit surface is this file plus the kernel's stable syscall contract. Built
//! natively per architecture by `scripts/build.ts` into the per-platform npm
//! packages (`@deepseek-ai/node-addon-landlock-run-linux-{x64,arm64}`); the
//! argv grammar, exit codes, and report lines are pinned in
//! `docs/cli-contract.md`.

use std::env;
use std::ffi::{CString, OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::process::ExitCode;

use libc::{c_int, c_long, c_uint, c_void};

/// The Landlock UAPI, defined locally instead of via a bindings crate: the
/// kernel's user-space ABI is stable by contract, self-defining it keeps the
/// build independent of dependency vintage, and the definitions double as the
/// audit record of exactly which kernel API this launcher touches. Layouts and
/// values are verbatim from the kernel header (the path-beneath struct is
/// packed there, so it must be packed here).
#[repr(C)]
struct LandlockRulesetAttr {
    handled_access_fs: u64,
}

#[repr(C, packed)]
struct LandlockPathBeneathAttr {
    allowed_access: u64,
    parent_fd: c_int,
}

const LANDLOCK_CREATE_RULESET_VERSION: c_uint = 1 << 0;
const LANDLOCK_RULE_PATH_BENEATH: c_int = 1;

/// Filesystem access bits, grouped by the Landlock ABI that introduced them.
// The complete access table is the audit record of the governed vocabulary;
// bits the launcher never names individually stay listed here anyway.
#[allow(dead_code)]
const LL_FS_EXECUTE: u64 = 1 << 0; // ABI 1
#[allow(dead_code)]
const LL_FS_WRITE_FILE: u64 = 1 << 1;
#[allow(dead_code)]
const LL_FS_READ_FILE: u64 = 1 << 2;
#[allow(dead_code)]
const LL_FS_READ_DIR: u64 = 1 << 3;
#[allow(dead_code)]
const LL_FS_REMOVE_DIR: u64 = 1 << 4;
#[allow(dead_code)]
const LL_FS_REMOVE_FILE: u64 = 1 << 5;
#[allow(dead_code)]
const LL_FS_MAKE_CHAR: u64 = 1 << 6;
#[allow(dead_code)]
const LL_FS_MAKE_DIR: u64 = 1 << 7;
#[allow(dead_code)]
const LL_FS_MAKE_REG: u64 = 1 << 8;
#[allow(dead_code)]
const LL_FS_MAKE_SOCK: u64 = 1 << 9;
#[allow(dead_code)]
const LL_FS_MAKE_FIFO: u64 = 1 << 10;
#[allow(dead_code)]
const LL_FS_MAKE_BLOCK: u64 = 1 << 11;
#[allow(dead_code)]
const LL_FS_MAKE_SYM: u64 = 1 << 12;
#[allow(dead_code)]
const LL_FS_REFER: u64 = 1 << 13; // ABI 2
#[allow(dead_code)]
const LL_FS_TRUNCATE: u64 = 1 << 14; // ABI 3 (ABI 4 added TCP bits only)
#[allow(dead_code)]
const LL_FS_IOCTL_DEV: u64 = 1 << 15; // ABI 5

/// Bits 0..12: every ABI-1 access, nothing newer.
const LL_ABI1_MASK: u64 = LL_FS_REFER - 1;

/// Newest ABI this build knows; the negotiation below scales the actual
/// ruleset down to what the running kernel supports.
const MAX_ABI: c_long = 5;

/// Landlock has no libc wrappers; these are the raw syscalls. The numbers are
/// identical on every architecture (the post-2011 unified table).
const NR_LANDLOCK_CREATE_RULESET: c_long = 444;
const NR_LANDLOCK_ADD_RULE: c_long = 445;
const NR_LANDLOCK_RESTRICT_SELF: c_long = 446;

/// Every fatal launcher error prints `landlock-run: <message>` to stderr and
/// exits 125 — a code the wrapped command itself is unlikely to use, so the
/// executor can tell launcher failures from command failures.
const EXIT_LAUNCHER_FAILURE: i32 = 125;
const FATAL_PREFIX: &str = "landlock-run: ";
const NOT_ENFORCED_MESSAGE: &str =
    "landlock is not enforced by this kernel (ABI unsupported or disabled)";

/// Access bits a non-directory grant keeps (the kernel rejects
/// directory-only accesses on a file rule with EINVAL).
const FILE_COMPATIBLE_ACCESS: u64 =
    LL_FS_EXECUTE | LL_FS_WRITE_FILE | LL_FS_READ_FILE | LL_FS_TRUNCATE | LL_FS_IOCTL_DEV;

/// The filesystem accesses the running kernel's ABI can govern.
fn fs_mask_for_abi(abi: c_long) -> u64 {
    let mut mask = LL_ABI1_MASK;
    if abi >= 2 {
        mask |= LL_FS_REFER;
    }
    if abi >= 3 {
        mask |= LL_FS_TRUNCATE;
    }
    if abi >= 5 {
        mask |= LL_FS_IOCTL_DEV;
    }
    mask
}

/// Print one fatal `landlock-run: ...` line; returns the fatal exit code.
fn fail(prefix: &str, detail: Option<&str>) -> ExitCode {
    match detail {
        Some(detail) => eprintln!("{FATAL_PREFIX}{prefix}: {detail}"),
        None => eprintln!("{FATAL_PREFIX}{prefix}"),
    }
    ExitCode::from(EXIT_LAUNCHER_FAILURE as u8)
}

/// Message and detail concatenate without a separator, like the contract's
/// `unknown argument: --bogus` and `--ro requires a path`.
fn fail_usage(message: &str, detail: Option<&str>) -> ExitCode {
    let line = match detail {
        Some(detail) => format!("{message}{detail}"),
        None => message.to_string(),
    };
    eprintln!("{FATAL_PREFIX}usage error: {line}");
    ExitCode::from(EXIT_LAUNCHER_FAILURE as u8)
}

/// libc's `strerror` text without std's ` (os error N)` suffix, keeping the
/// report lines identical to the contract's examples.
fn errno_text(err: &io::Error) -> String {
    let text = err.to_string();
    match text.find(" (os error ") {
        Some(cut) => text[..cut].to_string(),
        None => text,
    }
}

/// Parsed CLI: either a probe, or grants plus the command argv after `--`.
struct Cli {
    probe: bool,
    ro: Vec<OsString>,
    rw: Vec<OsString>,
    command: Vec<OsString>,
}

/// Hand-rolled argv parsing — four flags do not justify a parsing library.
/// Returns the parsed CLI, else the process exit code (message already
/// printed).
fn parse(args: &[OsString]) -> Result<Cli, ExitCode> {
    let mut cli = Cli { probe: false, ro: Vec::new(), rw: Vec::new(), command: Vec::new() };

    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_bytes() {
            b"--probe" => {
                if args.len() != 1 {
                    return Err(fail_usage("--probe takes no other arguments", None));
                }
                cli.probe = true;
                index += 1;
            }
            b"--ro" | b"--rw" => {
                if index + 1 >= args.len() {
                    let flag = arg.to_string_lossy().into_owned();
                    return Err(fail_usage(&flag, Some(" requires a path")));
                }
                if matches!(arg.as_bytes(), b"--ro") {
                    cli.ro.push(args[index + 1].clone());
                } else {
                    cli.rw.push(args[index + 1].clone());
                }
                index += 2;
            }
            b"--" => {
                cli.command = args[index + 1..].to_vec();
                break;
            }
            _ => {
                let unknown = arg.to_string_lossy().into_owned();
                return Err(fail_usage("unknown argument: ", Some(&unknown)));
            }
        }
    }
    if !cli.probe && cli.command.is_empty() {
        return Err(fail_usage("missing `-- <argv>...` command", None));
    }
    Ok(cli)
}

fn landlock_create_ruleset(attr: *const c_void, size: usize, flags: c_uint) -> io::Result<c_long> {
    let result = unsafe { libc::syscall(NR_LANDLOCK_CREATE_RULESET, attr, size, flags) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

/// Add one path-beneath rule.
fn add_rule(ruleset_fd: c_int, path: &OsStr, access: u64) -> Result<(), ExitCode> {
    let path_c = match CString::new(path.as_bytes()) {
        Ok(path_c) => path_c,
        Err(_) => return Err(fail("cannot open rule path", Some(&path.to_string_lossy()))),
    };
    let path_fd =
        unsafe { libc::open(path_c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if path_fd < 0 {
        // Fail closed on an unopenable grant root: silently narrowing the
        // granted set would be safe, but running with a profile the caller
        // did not get is not worth the ambiguity.
        let detail = errno_text(&io::Error::last_os_error());
        return Err(fail(
            &format!("cannot open rule path: {}", path.to_string_lossy()),
            Some(&detail),
        ));
    }
    // The kernel rejects directory-only accesses on a non-directory rule
    // (EINVAL), so a file grant keeps only the file-compatible bits — how the
    // `--rw /dev/null` grant works. A failed fstat keeps the full access,
    // matching the contract.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(path_fd, &mut stat) } == 0
        && stat.st_mode & libc::S_IFMT != libc::S_IFDIR
    {
        let masked = access & FILE_COMPATIBLE_ACCESS;
        return finish_add_rule(ruleset_fd, path_fd, masked);
    }
    finish_add_rule(ruleset_fd, path_fd, access)
}

fn finish_add_rule(ruleset_fd: c_int, path_fd: c_int, access: u64) -> Result<(), ExitCode> {
    let attr = LandlockPathBeneathAttr { allowed_access: access, parent_fd: path_fd };
    let result = unsafe {
        libc::syscall(
            NR_LANDLOCK_ADD_RULE,
            ruleset_fd,
            LANDLOCK_RULE_PATH_BENEATH,
            &attr as *const LandlockPathBeneathAttr as *const c_void,
            0,
        )
    };
    unsafe { libc::close(path_fd) };
    if result != 0 {
        let detail = errno_text(&io::Error::last_os_error());
        return Err(fail("landlock ruleset error", Some(&detail)));
    }
    Ok(())
}

/// Install the ruleset on the current thread, negotiating the kernel's ABI
/// down from MAX_ABI. `--ro` paths get the read side of the vocabulary (read
/// file/dir + execute — the wrapped `bash` and everything it spawns must
/// remain executable); `--rw` paths get every filesystem access the
/// negotiated ABI can grant. Sets `no_new_privs` first (mandatory for an
/// unprivileged restrict, and it neutralizes setuid/setgid escalation inside
/// the sandbox). On success sets whether the kernel governs only a subset of
/// MAX_ABI's accesses.
fn restrict_self(cli: &Cli, partial: &mut bool) -> Result<(), ExitCode> {
    let abi = match landlock_create_ruleset(std::ptr::null(), 0, LANDLOCK_CREATE_RULESET_VERSION) {
        Ok(abi) => abi,
        Err(_) => {
            // ENOSYS: kernel built without Landlock; EOPNOTSUPP: built but
            // disabled. Either way: not enforceable — fail CLOSED, never exec
            // unconfined.
            return Err(fail(NOT_ENFORCED_MESSAGE, None));
        }
    };
    *partial = abi < MAX_ABI;
    let handled = fs_mask_for_abi(if abi < MAX_ABI { abi } else { MAX_ABI });

    let attr = LandlockRulesetAttr { handled_access_fs: handled };
    let ruleset_fd = match landlock_create_ruleset(
        &attr as *const LandlockRulesetAttr as *const c_void,
        std::mem::size_of::<LandlockRulesetAttr>(),
        0,
    ) {
        Ok(fd) => fd as c_int,
        Err(err) => return Err(fail("landlock ruleset error", Some(&errno_text(&err)))),
    };

    let read_side = LL_FS_EXECUTE | LL_FS_READ_FILE | LL_FS_READ_DIR;
    for path in &cli.ro {
        add_rule(ruleset_fd, path, read_side & handled)?;
    }
    for path in &cli.rw {
        add_rule(ruleset_fd, path, handled)?;
    }

    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        let detail = errno_text(&io::Error::last_os_error());
        return Err(fail("landlock ruleset error", Some(&detail)));
    }
    let result = unsafe { libc::syscall(NR_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0) };
    unsafe { libc::close(ruleset_fd) };
    if result != 0 {
        let detail = errno_text(&io::Error::last_os_error());
        return Err(fail("landlock ruleset error", Some(&detail)));
    }
    Ok(())
}

/// Replace this process with the command. Returns only when exec fails.
fn exec(command: &[OsString]) -> ExitCode {
    let argv_c = match command
        .iter()
        .map(|arg| CString::new(arg.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(argv_c) => argv_c,
        // Kernel-delivered argv strings cannot contain interior NUL bytes;
        // treat the impossible conversion failure like any other failed exec.
        Err(_) => return fail("exec failed", Some("argument contains NUL")),
    };
    let mut argv: Vec<*const libc::c_char> = argv_c.iter().map(|arg| arg.as_ptr()).collect();
    argv.push(std::ptr::null());
    unsafe { libc::execvp(argv_c[0].as_ptr(), argv.as_ptr()) };
    // exec only returns on failure.
    let detail = errno_text(&io::Error::last_os_error());
    fail("exec failed", Some(&detail))
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let cli = match parse(&args) {
        Ok(cli) => cli,
        Err(code) => return code,
    };

    if cli.probe {
        // The functional probe: build and enforce a maximal ruleset in THIS
        // short-lived process (the probe run exits right after). `--version`
        // style checks would miss a kernel that has the syscalls but refuses
        // enforcement; actually restricting is the only honest signal. The one
        // report line is part of the launcher CLI contract — the executor
        // reads enforcement completeness from it.
        let probe_cli = Cli { probe: true, ro: vec![OsString::from("/")], rw: Vec::new(), command: Vec::new() };
        let mut partial = false;
        if let Err(code) = restrict_self(&probe_cli, &mut partial) {
            return code;
        }
        println!("landlock: {}", if partial { "partially enforced (older ABI)" } else { "fully enforced" });
        return ExitCode::SUCCESS;
    }

    let mut partial = false;
    if let Err(code) = restrict_self(&cli, &mut partial) {
        return code;
    }
    if partial {
        // Older ABI: some handled accesses are not governed (e.g. truncate
        // before ABI 3). Still confined for everything the kernel supports —
        // report, do not refuse.
        eprintln!("{FATAL_PREFIX}partial enforcement (older Landlock ABI)");
    }

    exec(&cli.command)
}
