//! Test-only: the device organ's environment, per the device pair amendment
//! PAIR-1.

use std::ffi::OsString;

use weaver_types::DeviceOrdinal;

use super::*;

fn inherited(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

fn strings(environment: &[CString]) -> Vec<String> {
    environment
        .iter()
        .map(|pair| pair.to_str().expect("ASCII fixture").to_string())
        .collect()
}

/// **The harness sets the mask from the binding**, replacing an inherited
/// value of each variable: an SPU fork whose worker holds another order and
/// another mask starts with the PCI bus order and the binding's ordinals in
/// binding order, each once, and every other variable as inherited.
///
/// Perturbation: keep an inherited value (append without filtering), or
/// sort the ordinals, and this test fails.
#[test]
fn the_mask_replaces_inherited_values_in_binding_order() {
    let environment = organ_environment(
        inherited(&[
            ("CUDA_DEVICE_ORDER", "FASTEST_FIRST"),
            ("HOME", "/var/lib/weaver-agent"),
            ("CUDA_VISIBLE_DEVICES", "2"),
        ]),
        &device_environment(&[DeviceOrdinal(1), DeviceOrdinal(0)]),
    )
    .expect("an environment");
    assert_eq!(
        strings(&environment),
        [
            "HOME=/var/lib/weaver-agent",
            "CUDA_DEVICE_ORDER=PCI_BUS_ID",
            "CUDA_VISIBLE_DEVICES=1,0",
        ]
    );
}

/// No override leaves the inherited environment as it was, which is the
/// gate's launch.
#[test]
fn no_override_keeps_the_inherited_environment() {
    let pairs = [("CUDA_VISIBLE_DEVICES", "2"), ("HOME", "/h")];
    let environment = organ_environment(inherited(&pairs), &[]).expect("an environment");
    assert_eq!(strings(&environment), ["CUDA_VISIBLE_DEVICES=2", "HOME=/h"]);
}

/// An override that cannot cross `execve` refuses the fork rather than
/// being dropped.
#[test]
fn an_unrepresentable_override_refuses() {
    assert_eq!(
        organ_environment(inherited(&[]), &[("X", "a\0b".to_string())]),
        Err(nix::Error::EINVAL)
    );
}

/// **The organ sees the override through the real fork**: a shell reports
/// both variables as its last typed line, set to the binding's values
/// whatever the test process holds.
///
/// Perturbation: hand `execve` the inherited environment alone and this
/// test fails.
#[test]
fn a_forked_organ_sees_the_mask() {
    let (word, write_end) = LastWord::stand().expect("a pipe");
    let pid = unsafe {
        fork_organ(
            std::path::Path::new("/bin/sh"),
            &[],
            &[
                "-c".to_string(),
                r#"printf '{"order":"%s","visible":"%s"}\n' "$CUDA_DEVICE_ORDER" "$CUDA_VISIBLE_DEVICES" >&2"#
                    .to_string(),
            ],
            &device_environment(&[DeviceOrdinal(3), DeviceOrdinal(1)]),
            Some(std::os::fd::AsRawFd::as_raw_fd(&write_end)),
        )
    }
    .expect("the fork");
    drop(write_end);
    let _ = nix::sys::wait::waitpid(pid, None);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let line = loop {
        if let Some(line) = word.take() {
            break line;
        }
        assert!(std::time::Instant::now() < deadline, "no word arrived");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(line, r#"{"order":"PCI_BUS_ID","visible":"3,1"}"#);
}
