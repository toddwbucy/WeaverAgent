//! The launcher's walls, read from the program and the tables it lays. The
//! walls laid on a real interpreter are `tests/loop_launch.rs`.

use super::*;

/// The calls the filter refuses, by the kernel's numbers.
const REFUSED: [(&str, i64); 7] = [
    ("io_uring_setup", libc::SYS_io_uring_setup),
    ("socket", libc::SYS_socket),
    ("socketpair", libc::SYS_socketpair),
    ("clone", libc::SYS_clone),
    ("fork", libc::SYS_fork),
    ("vfork", libc::SYS_vfork),
    ("clone3", libc::SYS_clone3),
];

/// The filter's verdict on one call, by [`LOOP_FILTER`]'s own instructions
/// run over a `seccomp_data` as the kernel runs them.
fn verdict(arch: u32, nr: u32) -> u32 {
    let mut data = [0u8; 64];
    data[0..4].copy_from_slice(&nr.to_le_bytes());
    data[4..8].copy_from_slice(&arch.to_le_bytes());
    let mut accumulator = 0u32;
    let mut next = 0usize;
    loop {
        let SockFilter { code, jt, jf, k } = LOOP_FILTER[next];
        next += 1;
        match code {
            BPF_LD_W_ABS => {
                let at = k as usize;
                accumulator = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
            }
            BPF_JEQ_K => next += usize::from(if accumulator == k { jt } else { jf }),
            BPF_JGE_K => next += usize::from(if accumulator >= k { jt } else { jf }),
            BPF_RET_K => return k,
            other => panic!("an instruction this reader does not know: {other:#x}"),
        }
    }
}

/// **Each named call is refused with `EACCES`, and every other call of
/// x86-64 is allowed.** Perturbation: drop a row, or point a jump one short,
/// and a named call is allowed or its neighbour refused.
#[test]
fn the_filter_refuses_the_named_calls_and_allows_the_rest() {
    for (name, nr) in REFUSED {
        assert_eq!(
            verdict(AUDIT_ARCH_X86_64, nr as u32),
            DENY,
            "{name} must be refused"
        );
    }
    assert_eq!(DENY, SECCOMP_RET_ERRNO | 13, "the refusal is EACCES");
    let refused: Vec<u32> = REFUSED.iter().map(|(_, nr)| *nr as u32).collect();
    for nr in 0..600u32 {
        if refused.contains(&nr) {
            continue;
        }
        assert_eq!(
            verdict(AUDIT_ARCH_X86_64, nr),
            SECCOMP_RET_ALLOW,
            "call {nr} must be allowed"
        );
    }
}

/// **Another architecture is refused whole, and every x32 call.**
/// Perturbation: drop the architecture rows or the x32 rows.
#[test]
fn the_filter_refuses_x32_and_another_architecture() {
    const AUDIT_ARCH_I386: u32 = 0x4000_0003;
    assert_eq!(verdict(AUDIT_ARCH_I386, libc::SYS_read as u32), DENY);
    assert_eq!(
        verdict(AUDIT_ARCH_X86_64, X32_SYSCALL_BIT | libc::SYS_read as u32),
        DENY
    );
    assert_eq!(
        verdict(AUDIT_ARCH_X86_64, libc::SYS_read as u32),
        SECCOMP_RET_ALLOW
    );
}

/// **The limits are the shell's values**, soft and hard alike, in the
/// shell's order. Perturbation: change a value or drop a row.
#[test]
fn the_limits_are_the_shells_values() {
    assert_eq!(
        RLIMITS,
        [
            (libc::RLIMIT_FSIZE, 64 << 20),
            (libc::RLIMIT_NOFILE, 64),
            (libc::RLIMIT_AS, 512 << 20),
            (libc::RLIMIT_CORE, 0),
        ]
    );
}

/// **The ABI floor fails closed**: a failed query answers as one below it.
#[test]
fn the_abi_floor_fails_closed() {
    assert_eq!(abi_at_floor(Ok(6)), Ok(6));
    assert_eq!(abi_at_floor(Ok(11)), Ok(11));
    assert!(
        abi_at_floor(Ok(5))
            .unwrap_err()
            .contains("below the floor of 6")
    );
    assert_eq!(
        abi_at_floor(Err(libc::ENOSYS)).unwrap_err(),
        "the Landlock ABI query failed"
    );
}

/// **A granted path that is a link refuses**, judged on its descriptor.
#[test]
fn a_link_is_refused_where_a_rule_would_take_it() {
    let dir = std::env::temp_dir().join(format!("weaver-loop-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("file");
    std::fs::write(&file, b"x").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    let ok = open_judged(&file, |_| None);
    let refused = open_judged(&link, |_| None).err();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok.is_ok());
    assert_eq!(refused, Some(format!("{} is a link", link.display())));
}
