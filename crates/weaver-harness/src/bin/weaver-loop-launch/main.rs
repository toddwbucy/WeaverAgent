//! `weaver-loop-launch`, the loop's launcher, per `weaver-harness-Spec`
//! section 2.5: the worker forks it by the organ fork as it stands, with the
//! loop's end at descriptor 3 and the last word's pipe at 2. With one thread
//! it then:
//!
//! 1. clears the environment to one pair, `LANG=C.UTF-8`, the one variable the
//!    interpreter is executed with
//! 2. opens the null device and places it at 0 and 1
//! 3. reads the grant from its root-owned file and judges every path in it,
//!    the interpreter's sha256 compared with the grant's
//! 4. lays no-new-privs, the limits, the filter and Landlock on itself, in
//!    the shell's order
//! 5. executes the interpreter on the runner and the wire file.
//!
//! Any step that fails ends it before its exec with one JSON line on standard
//! error naming why, `{"loop": "launch", "reason": ...}`, so the last word
//! keeps it, and no runner starts with a wall missing.
//!
//! Its arguments are two absolute paths, the grant file and the wire file.
//! The runner is `runner.py` in the grant file's folder, and the loop folder
//! granted is the wire file's.

mod confine;
mod grant;

use std::ffi::{CString, OsString};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nix::libc;

/// The one variable the interpreter is executed with.
const ENVIRONMENT: &str = "LANG=C.UTF-8";

/// The uid that owns the grant file, the runner's folder and the pycache
/// folder: root, so the agent's uid cannot widen its own walls.
const OWNER: u32 = 0;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let reason = match launch(&args) {
        Ok(never) => match never {},
        Err(reason) => reason,
    };
    let line = serde_json::json!({ "loop": "launch", "reason": reason });
    eprintln!("{line}");
    ExitCode::FAILURE
}

/// The launch, which returns only with the reason it ended before its exec.
fn launch(args: &[OsString]) -> Result<std::convert::Infallible, String> {
    let [grant_file, wire_file] = args else {
        return Err("usage: weaver-loop-launch <grant file> <wire file>".to_string());
    };
    let grant_file = absolute(grant_file, "the grant file")?;
    let wire_file = absolute(wire_file, "the wire file")?;
    let runner_dir = grant_file
        .parent()
        .ok_or_else(|| "the grant file has no folder".to_string())?;
    let loop_dir = wire_file
        .parent()
        .ok_or_else(|| "the wire file has no folder".to_string())?;
    the_loop_end_at_3()?;
    null_at_0_and_1()?;
    let grant = grant::read_grant_file(&grant_file, OWNER)?;
    let rules = grant::judge(
        &grant,
        &grant::Launch {
            runner_dir,
            loop_dir,
        },
        OWNER,
    )?;
    let abi = confine::abi_at_floor(confine::landlock_abi())?;
    confine::rlimits_within_reach()?;
    let ruleset = confine::build_ruleset(abi, &rules)?;
    drop(rules);
    // **Exactly 0 to 3 cross the exec**: every descriptor above 3 is marked
    // close-on-exec, whatever the fork left open.
    // SAFETY: close_range on a range, closing nothing now.
    if unsafe {
        libc::syscall(
            libc::SYS_close_range,
            4u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    } != 0
    {
        return Err(format!("close_range: {}", std::io::Error::last_os_error()));
    }
    let argv = command(&grant, runner_dir, &wire_file)?;
    let envp = [CString::new(ENVIRONMENT).expect("no NUL")];
    let interpreter = c_path(&grant.interpreter)?;
    confine::apply(&ruleset)?;
    let errno = match nix::unistd::execve(&interpreter, &argv, &envp) {
        Ok(never) => match never {},
        Err(errno) => errno,
    };
    Err(format!(
        "the interpreter {} did not execute: {errno}",
        grant.interpreter.display()
    ))
}

/// **The runner's command**: the interpreter isolated (`-I`), writing no
/// bytecode (`-B`), without `site` (`-S`), its bytecode reads pointed at the
/// empty pycache folder, then the runner and the wire file.
fn command(
    grant: &grant::Grant,
    runner_dir: &Path,
    wire_file: &Path,
) -> Result<Vec<CString>, String> {
    let mut prefix = OsString::from("pycache_prefix=");
    prefix.push(grant.pycache.as_os_str());
    let mut argv = vec![c_path(&grant.interpreter)?];
    for flag in ["-I", "-B", "-S", "-X"] {
        argv.push(CString::new(flag).expect("no NUL"));
    }
    argv.push(c_path(Path::new(&prefix))?);
    argv.push(c_path(&runner_dir.join("runner.py"))?);
    argv.push(c_path(wire_file)?);
    Ok(argv)
}

fn c_path(path: &Path) -> Result<CString, String> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| format!("{} holds a NUL", path.display()))
}

fn absolute(arg: &OsString, what: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(arg);
    if !path.is_absolute() {
        return Err(format!("{what} {} is not absolute", path.display()));
    }
    Ok(path)
}

/// **Descriptor 3 is the loop's end**, a socket the organ fork placed. A
/// launch without it would start a runner that cannot speak.
fn the_loop_end_at_3() -> Result<(), String> {
    // SAFETY: fstat on a descriptor number into a value this frame holds.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(3, &mut stat) } != 0 {
        return Err(format!(
            "descriptor 3, the loop's end, is not open: {}",
            std::io::Error::last_os_error()
        ));
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFSOCK {
        return Err("descriptor 3, the loop's end, is not a socket".to_string());
    }
    Ok(())
}

/// **The null device at 0 and 1**, opened before Landlock is laid, so the
/// grant does not name it, and the runner holds no log of the worker's open.
fn null_at_0_and_1() -> Result<(), String> {
    let null = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .map_err(|e| format!("the null device does not open: {e}"))?;
    let fd = std::os::fd::AsRawFd::as_raw_fd(&null);
    for target in [0, 1] {
        // SAFETY: dup2 onto a standard descriptor this process owns. The copy
        // is born without close-on-exec, so it crosses the exec.
        if unsafe { libc::dup2(fd, target) } != target {
            return Err(format!(
                "the null device does not go to {target}: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}
