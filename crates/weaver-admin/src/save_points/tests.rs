/// **The marker's temporary is created narrow** (the custody audit's
/// G14): read right after its creation, before its owner and mode are
/// set, it carries no group or other bit, so a crash there leaves nothing
/// a later verb's judgment of the root would refuse. Perturbation: create
/// it with `File::create` again and it carries the umask's bits.
#[test]
fn the_marker_temporary_is_created_narrow() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-marker-narrow-{}", std::process::id())),
    );
    let root = scratch.0.clone();
    std::fs::create_dir_all(&root).unwrap();
    let mut seen = None;
    write_marker_with(
        &root,
        Some(&Marker::Open { run: "r-1".into() }),
        &mut |file| {
            use std::os::unix::fs::PermissionsExt;
            seen = Some(file.metadata().unwrap().permissions().mode() & 0o777);
        },
    )
    .unwrap();
    let mode = seen.expect("the hook ran");
    assert_eq!(mode & 0o077, 0, "created at {mode:o}");
    assert_eq!(read_marker(&root), Some(Marker::Open { run: "r-1".into() }));
}

/// **A marker that stands and does not read fails closed** (the custody
/// audit's G8): every arm of `load_marker` that finds a marker it cannot
/// read is an error, never no marker and never another marker; absent
/// alone is none. Each damaged marker but the torn one would read as a
/// marker were its arm gone: the unknown state, the missing run and the
/// non-UTF-8 run are whole JSON, the link points at a sound marker, and
/// the marker past the bound is a sound one padded with whitespace.
/// Perturbations, one per arm: read the torn one as absent; read an
/// unknown state as absent; read an open that fails, the link's `ELOOP`
/// among them, as absent; read a missing run as the empty run; read the
/// bytes lossily as text; read past the bound as a closed marker. Each
/// fails the case it names.
#[test]
fn a_marker_that_does_not_read_is_an_error() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-marker-torn-{}", std::process::id())),
    );
    let root = scratch.0.clone();
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(load_marker(&root), Ok(None), "absent is none");
    let sound = br#"{"run":"r-1","state":"closed"}"#;
    let mut past_bound = sound.to_vec();
    past_bound.resize(MARKER_BOUND as usize + 1, b' ');
    let mut not_text = br#"{"run":"r-"#.to_vec();
    not_text.extend_from_slice(&[0xff, 0xfe]);
    not_text.extend_from_slice(br#"","state":"open"}"#);
    let cases: [(&str, Vec<u8>); 5] = [
        ("a torn marker", br#"{"run":"#.to_vec()),
        (
            "an unknown state",
            br#"{"run":"r-1","state":"bogus"}"#.to_vec(),
        ),
        ("a missing run", br#"{"state":"open"}"#.to_vec()),
        ("bytes that are not UTF-8", not_text),
        ("a marker past its bound", past_bound),
    ];
    for (what, bytes) in cases {
        std::fs::write(root.join(MARKER), &bytes).unwrap();
        assert!(
            load_marker(&root).is_err(),
            "{what}: expected an error, read {:?}",
            load_marker(&root)
        );
    }
    // A link at the marker's name, to a sound marker, is never followed.
    std::fs::remove_file(root.join(MARKER)).unwrap();
    std::fs::write(root.join("elsewhere"), sound).unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), root.join(MARKER)).unwrap();
    assert!(
        load_marker(&root).is_err(),
        "a link is an error, read {:?}",
        load_marker(&root)
    );
    // The sound marker itself reads, so each case above failed on its
    // own arm.
    std::fs::remove_file(root.join(MARKER)).unwrap();
    std::fs::write(root.join(MARKER), sound).unwrap();
    assert_eq!(
        load_marker(&root),
        Ok(Some(Marker::Closed { run: "r-1".into() }))
    );
}

/// **A manifest whose creation does not finish is unmade** (#99, W1;
/// #107): a chown that fails after the exclusive create refuses and
/// leaves no manifest, so the next append creates one afresh rather
/// than meeting an empty file every later publication would refuse.
/// Perturbation: drop the unlink and the manifest stands empty after the
/// refusal, and the second append meets it.
#[test]
fn a_manifest_whose_creation_fails_is_unmade_and_the_next_append_succeeds() {
    let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-manifest-unmade-{}",
        std::process::id()
    )));
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let line = ManifestLine {
        ordinal: 1,
        digest: "ab".into(),
        name: "ab.save-point".into(),
        stamp: Stamp {
            run: "r-1".into(),
            sequence: 1,
            turn: 0,
            schema: String::new(),
            wall_ns: 0,
        },
        position: None,
        arrived: Arrival::Leave,
    };
    let mut failing =
        |_: BorrowedFd<'_>, _: u32, _: u32| -> nix::Result<()> { Err(nix::errno::Errno::EPERM) };
    assert_eq!(
        append_line_with(dir_fd.as_fd(), mine, &line, &mut failing),
        Err(LifecycleRefusal::BoundaryUnverified),
        "a chown that fails refuses"
    );
    assert!(
        !entry_stands(dir_fd.as_fd(), MANIFEST),
        "and leaves no manifest behind"
    );
    let mut fine = |_: BorrowedFd<'_>, _: u32, _: u32| -> nix::Result<()> { Ok(()) };
    append_line_with(dir_fd.as_fd(), mine, &line, &mut fine)
        .expect("the next append creates the manifest afresh");
    assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 1);
}

/// **A new manifest takes its owner and group explicitly** (Codex on
/// #94): the first line creates the manifest and sets its owner and
/// group to the expected owner's, whatever the creating process's
/// effective gid, before its mode and the directory's sync; a later line,
/// the manifest standing, sets nothing. The chown is counted through the
/// seam, so that case needs no second group and never skips; the
/// production closure in `append_line` is then pinned against a
/// supplementary group of this test's user, which an unprivileged
/// process may `fchown` its own file to, so the manifest's group can
/// only be that group if the closure ran. A box whose test user holds
/// no second group skips that half, saying so. Perturbations: drop the
/// call and the count reads zero; make `append_line`'s closure do
/// nothing and the new manifest keeps the effective gid.
#[test]
fn a_new_manifest_takes_its_owner_and_group_explicitly() {
    let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-manifest-owner-{}",
        std::process::id()
    )));
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let line = ManifestLine {
        ordinal: 1,
        digest: "ab".into(),
        name: "ab.save-point".into(),
        stamp: Stamp {
            run: "r-1".into(),
            sequence: 1,
            turn: 0,
            schema: String::new(),
            wall_ns: 0,
        },
        position: None,
        arrived: Arrival::Leave,
    };
    let mut chowned = Vec::new();
    let mut counting = |_: BorrowedFd<'_>, uid: u32, gid: u32| -> nix::Result<()> {
        chowned.push((uid, gid));
        Ok(())
    };
    append_line_with(dir_fd.as_fd(), mine, &line, &mut counting).unwrap();
    let second = ManifestLine {
        ordinal: 2,
        ..line.clone()
    };
    append_line_with(dir_fd.as_fd(), mine, &second, &mut counting).unwrap();
    assert_eq!(
        chowned,
        [(mine.uid, mine.gid)],
        "set once, at the creation, to the expected owner"
    );
    assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 2);
    // The production closure: a group this user holds that is not its
    // effective gid, which a new file would take without the chown.
    let egid = nix::unistd::getegid();
    let Some(other) = nix::unistd::getgroups()
        .unwrap()
        .into_iter()
        .find(|group| *group != egid)
    else {
        eprintln!(
            "skipped: this test's user holds no group beside its effective gid, so the production chown cannot be told from none"
        );
        return;
    };
    let grouped = dir.join("grouped");
    std::fs::create_dir_all(&grouped).unwrap();
    let grouped_fd = open_directory(&grouped).unwrap();
    let theirs = Owner {
        uid: mine.uid,
        gid: other.as_raw(),
    };
    append_line(grouped_fd.as_fd(), theirs, &line).unwrap();
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(grouped.join(MANIFEST)).unwrap().gid(),
            other.as_raw(),
            "the new manifest takes the owner's group, not the effective gid"
        );
    }
    assert_eq!(read_manifest(grouped_fd.as_fd(), theirs).unwrap().len(), 1);
}

/// **An adopted entry is judged, then made durable** (Codex on #94): an
/// entry that stands is judged as a load judges it and the directory
/// synced once before any manifest line can name it; a name with no
/// entry is answered none and syncs nothing. The sync is counted through
/// the helper's seam. Perturbation: drop the sync and the count reads
/// zero for the standing entry.
#[test]
fn an_adopted_entry_is_judged_then_synced() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-adopt-durably-{}", std::process::id())),
    );
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let mine = (
        nix::unistd::getuid().as_raw(),
        nix::unistd::getgid().as_raw(),
    );
    let bytes = save_point("r-8", 8, 0, 8_000_000_000, b"adopted");
    let name = published_name(&judge(&bytes).unwrap());
    std::fs::write(dir.join(&name), &bytes).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    let mut syncs = 0;
    let mut counting = |_: BorrowedFd<'_>| -> std::io::Result<()> {
        syncs += 1;
        Ok(())
    };
    let adopted = adopt_durably(dir_fd.as_fd(), &name, mine, &mut counting)
        .expect("the standing entry judges")
        .expect("it stands");
    assert_eq!(adopted.1.digest, judge(&bytes).unwrap().digest);
    assert!(
        adopt_durably(dir_fd.as_fd(), "absent.save-point", mine, &mut counting)
            .unwrap()
            .is_none()
    );
    assert_eq!(syncs, 1, "synced once, for the entry that stands");
}

/// **A published file is judged by its group as by its owner** (Codex on
/// #94): a save point that stands under another group than the access
/// group, which the operator and the connector could not read, is refused
/// by `restore` naming it and by a load selecting it; the same file
/// judged against its own group reads. The file is written under this
/// test's own uid and gid and the judgment given a gid one off, so no
/// supplementary group is needed and the case never skips. Perturbation:
/// drop the gid comparison and the wrong group reads.
#[test]
fn a_published_file_under_another_group_is_refused() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-group-{}", std::process::id())),
    );
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let wrong = (mine.uid, mine.gid.wrapping_add(1));
    let bytes = save_point("r-7", 7, 0, 7_000_000_000, b"grouped");
    let name = published_name(&judge(&bytes).unwrap());
    std::fs::write(dir.join(&name), &bytes).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    assert!(
        name_at_restore(dir_fd.as_fd(), wrong, &name, mine).is_err(),
        "restore refuses a file under another group"
    );
    let named = name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &name, mine)
        .expect("its own group reads");
    assert!(
        select(dir_fd.as_fd(), wrong, Some(&named.name), mine).is_err(),
        "a load refuses it under another group"
    );
    assert!(
        select(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            Some(&named.name),
            mine
        )
        .unwrap()
        .is_some(),
        "and selects it under its own"
    );
}

/// **A published file carrying an access entry beyond its mode is
/// refused** (#99 area 2, H5; #107): the same file without the entry
/// reads. Skips, naming why, where setfacl cannot set the entry.
/// Perturbation: drop the look from `open_judged` and the entry reads.
#[test]
fn a_published_file_carrying_an_access_entry_is_refused() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-published-acl-{}", std::process::id())),
    );
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let bytes = save_point("r-8", 8, 0, 8_000_000_000, b"entry");
    let name = published_name(&judge(&bytes).unwrap());
    let path = dir.join(&name);
    std::fs::write(&path, &bytes).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    let set = std::process::Command::new("setfacl")
        .args(["-m", "u:nobody:r"])
        .arg(&path)
        .status();
    if !set.is_ok_and(|status| status.success()) {
        eprintln!("SKIP: setfacl could not set an entry here");
        return;
    }
    assert!(
        open_judged(dir_fd.as_fd(), &name, (mine.uid, mine.gid)).is_err(),
        "a save point another principal could read through an entry"
    );
    let cleared = std::process::Command::new("setfacl")
        .arg("-b")
        .arg(&path)
        .status();
    assert!(cleared.is_ok_and(|status| status.success()));
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    assert!(
        open_judged(dir_fd.as_fd(), &name, (mine.uid, mine.gid))
            .unwrap()
            .is_some(),
        "and without it the file reads"
    );
}

/// **The size bound holds through the read** (Codex on #94): a source
/// that ends within the bound reads whole, one that reaches past it is
/// refused, however long it runs, as a room file its member grows after
/// the length check would, and reads no more of it than one byte past
/// the bound. The bound is a parameter here so the test needs no
/// gibibyte. Perturbation: read without the `take` and the whole long
/// source is pulled into memory.
#[test]
fn a_read_past_the_bound_is_refused_however_long_the_source() {
    let bound = 16u64;
    let mut within = std::io::Cursor::new(vec![7u8; 16]);
    assert_eq!(
        read_within(&mut within, bound).unwrap(),
        Some(vec![7u8; 16])
    );
    let mut past = std::io::Cursor::new(vec![7u8; 17]);
    assert_eq!(read_within(&mut past, bound).unwrap(), None);
    // A source far longer than the bound, as a file its member keeps
    // growing: the read takes at most one byte past the bound.
    struct Counting {
        left: u64,
        pulled: u64,
    }
    impl Read for Counting {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let n = (buffer.len() as u64).min(self.left) as usize;
            buffer[..n].fill(7);
            self.left -= n as u64;
            self.pulled += n as u64;
            Ok(n)
        }
    }
    let mut long = Counting {
        left: 1 << 20,
        pulled: 0,
    };
    assert_eq!(read_within(&mut long, bound).unwrap(), None);
    assert!(
        long.pulled <= bound + 1,
        "read {} bytes of a source past a bound of {bound}",
        long.pulled
    );
}

use super::*;

/// A save point in the member's format, built here as the member builds
/// one, so the judgment below reads what the member writes.
pub(crate) fn save_point(
    run: &str,
    sequence: u64,
    turn: u64,
    wall_ns: u64,
    image: &[u8],
) -> Vec<u8> {
    // One member process whose count follows the covered position, as
    // a member that took them in order would have.
    save_point_taken(run, sequence, turn, wall_ns, image, 1, sequence)
}

/// A save point as `save_point` builds it, taken by member process `pid`
/// as its `ordinal`th.
pub(crate) fn save_point_taken(
    run: &str,
    sequence: u64,
    turn: u64,
    wall_ns: u64,
    image: &[u8],
    pid: u64,
    ordinal: u64,
) -> Vec<u8> {
    let header = serde_json::json!({
        "weaver-save-point": 1,
        "run": run,
        "sequence": sequence,
        "turn": turn,
        "schema": "0".repeat(64),
        "image": image.len(),
        "taken": {"pid": pid, "ordinal": ordinal, "wall_ns": wall_ns.to_string()},
    })
    .to_string();
    let mut hasher = sha2::Sha256::new();
    hasher.update(header.as_bytes());
    hasher.update(b"\n");
    hasher.update(image);
    let check = serde_json::json!({"check": hex(&hasher.finalize())}).to_string();
    let mut out = header.into_bytes();
    out.push(b'\n');
    out.extend_from_slice(check.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(image);
    out
}

/// **The judgment reads the member's format and refuses what is not
/// it**, per `weaver-state-Spec` section 3: a sound file answers its
/// stamp and digest, and a torn image, a flipped byte, a stamp with a
/// member missing and a check that does not hold each refuse naming
/// why. Perturbation: skip the check's comparison and the flipped byte
/// judges sound.
#[test]
fn the_judgment_reads_the_format_and_refuses_damage() {
    let bytes = save_point("r-1", 41, 2, 1_759_788_000_000_000_000, b"image bytes");
    let judged = judge(&bytes).expect("sound");
    assert_eq!(
        (
            judged.stamp.run.as_str(),
            judged.stamp.sequence,
            judged.stamp.turn
        ),
        ("r-1", 41, 2)
    );
    assert_eq!(judged.digest, hex(&sha2::Sha256::digest(&bytes)));
    assert_eq!(
        published_name(&judged),
        format!("20251006T220000Z-{}.save-point", judged.digest)
    );
    let mut flipped = bytes.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    assert!(judge(&flipped).unwrap_err().contains("check"));
    assert!(
        judge(&bytes[..bytes.len() - 3])
            .unwrap_err()
            .contains("bytes")
    );
    let short_stamp = bytes.to_vec();
    let header_end = short_stamp.iter().position(|&b| b == b'\n').unwrap();
    let mut header: serde_json::Value = serde_json::from_slice(&short_stamp[..header_end]).unwrap();
    header.as_object_mut().unwrap().remove("turn");
    let mut rebuilt = header.to_string().into_bytes();
    rebuilt.extend_from_slice(&short_stamp[header_end..]);
    assert!(judge(&rebuilt).unwrap_err().contains("seven"));
    // The nonce is judged as the member judges it: three members of the
    // wrong names, or a pid that is not a number, refuse (Codex on #94,
    // round 1). Perturbation: drop the pid and ordinal checks and both
    // judge sound.
    for taken in [
        r#"{"x":1,"y":2,"wall_ns":"1"}"#,
        r#"{"pid":"one","ordinal":0,"wall_ns":"1"}"#,
    ] {
        let mut header: serde_json::Value = serde_json::from_slice(&bytes[..header_end]).unwrap();
        header["taken"] = serde_json::from_str(taken).unwrap();
        let mut rebuilt = header.to_string().into_bytes();
        rebuilt.extend_from_slice(&bytes[header_end..]);
        let why = judge(&rebuilt).unwrap_err();
        assert!(why.contains("taken"), "{taken}: {why}");
    }
}

/// **Several room entries publish in the stamp's order**, per Spec
/// section 6 (Codex on #94, round 5): an older file left unreported and
/// a newer one reported in the same verb take their ordinals by kind and
/// then the member process's count, so the newer is the latest the
/// manifest names whatever order the room lists them in. The newer's
/// digest is chosen to sort first, which is the order the room reader
/// yields. Perturbation: drop the sort in `publish` and the older file
/// is minted last and selected as latest.
#[test]
fn several_room_entries_publish_in_the_stamps_order() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-order-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("decl");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let owner = (me, mine.gid);
    let older = save_point("r-1", 7, 1, 5_000_000_000, b"older, unreported");
    let older_digest = judge(&older).unwrap().digest;
    let (newer, newer_digest) = (0u32..)
        .map(|i| {
            let bytes = save_point("r-1", 9, 2, 6_000_000_000, format!("newer {i}").as_bytes());
            let digest = judge(&bytes).unwrap().digest;
            (bytes, digest)
        })
        .find(|(_, digest)| digest < &older_digest)
        .expect("a newer file whose digest sorts first");
    std::fs::write(room.join(format!("{newer_digest}{SUFFIX}")), &newer).unwrap();
    std::fs::write(room.join(format!("{older_digest}{SUFFIX}")), &older).unwrap();
    let report = weaver_types::SavePointReport {
        save_point: newer_digest.clone(),
        name: format!("{newer_digest}{SUFFIX}"),
        run: weaver_types::RunId("r-1".into()),
        sequence: 9,
        turn: 2,
        event_run: weaver_types::RunId("r-2".into()),
        position: 9,
    };
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[(report, Arrival::Demand)],
    )
    .unwrap();
    let minted: Vec<(u64, &str, Arrival)> = lines
        .iter()
        .map(|line| (line.ordinal, line.digest.as_str(), line.arrived))
        .collect();
    assert_eq!(
        minted,
        vec![
            (1, older_digest.as_str(), Arrival::Recovered),
            (2, newer_digest.as_str(), Arrival::Demand),
        ],
        "ordinals follow the sequence, the reported one last"
    );
    let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
        .unwrap()
        .expect("a latest");
    assert_eq!(latest.line.digest, newer_digest);
    // The manifest's position is the event's run and sequence, never
    // the covered position's run (Codex on #94, round 6). Perturbation:
    // write `report.run` into the position and this reads r-1.
    assert_eq!(lines[1].position, Some(("r-2".into(), 9)));
    // **A reported save point is last whatever the clock did** (Codex
    // on #94, round 6): a recovered file whose clock is ahead of the
    // reported one's, as a clock stepped back between them leaves it,
    // is still minted below. Perturbation: sort by the clock alone and
    // the reported one is minted first, the recovered selected latest.
    let room = base.join("room-2");
    let dir = base.join("decl-2");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let ahead = save_point("r-1", 3, 1, 8_000_000_000, b"recovered, clock ahead");
    let ahead_digest = judge(&ahead).unwrap().digest;
    let behind = save_point("r-1", 4, 1, 7_000_000_000, b"reported, clock behind");
    let behind_digest = judge(&behind).unwrap().digest;
    std::fs::write(room.join(format!("{ahead_digest}{SUFFIX}")), &ahead).unwrap();
    std::fs::write(room.join(format!("{behind_digest}{SUFFIX}")), &behind).unwrap();
    let report = weaver_types::SavePointReport {
        save_point: behind_digest.clone(),
        name: format!("{behind_digest}{SUFFIX}"),
        run: weaver_types::RunId("r-1".into()),
        sequence: 4,
        turn: 1,
        event_run: weaver_types::RunId("r-1".into()),
        position: 4,
    };
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[(report, Arrival::Demand)],
    )
    .unwrap();
    let minted: Vec<(u64, &str)> = lines
        .iter()
        .map(|line| (line.ordinal, line.digest.as_str()))
        .collect();
    assert_eq!(
        minted,
        vec![(1, ahead_digest.as_str()), (2, behind_digest.as_str())],
        "the reported one is last though its clock is behind"
    );
    let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
        .unwrap()
        .expect("a latest");
    assert_eq!(latest.line.digest, behind_digest);
    // **The reported key orders ahead of the count** (the #99 area 1
    // review): the reported save point is minted last even where its
    // `taken.ordinal` is below a recovered one's, so "reported last" is
    // pinned on its own and not through the count agreeing with it.
    // Perturbation: sort by the count alone and the recovered one is
    // minted last and selected latest.
    let room = base.join("room-3");
    let dir = base.join("decl-3");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let counted_high = save_point_taken("r-1", 9, 2, 9_000_000_000, b"recovered, count 9", 1, 9);
    let counted_high_digest = judge(&counted_high).unwrap().digest;
    let counted_low = save_point_taken("r-1", 4, 1, 4_000_000_000, b"reported, count 4", 1, 4);
    let counted_low_digest = judge(&counted_low).unwrap().digest;
    std::fs::write(
        room.join(format!("{counted_high_digest}{SUFFIX}")),
        &counted_high,
    )
    .unwrap();
    std::fs::write(
        room.join(format!("{counted_low_digest}{SUFFIX}")),
        &counted_low,
    )
    .unwrap();
    let report = weaver_types::SavePointReport {
        save_point: counted_low_digest.clone(),
        name: format!("{counted_low_digest}{SUFFIX}"),
        run: weaver_types::RunId("r-1".into()),
        sequence: 4,
        turn: 1,
        event_run: weaver_types::RunId("r-1".into()),
        position: 4,
    };
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[(report, Arrival::Leave)],
    )
    .unwrap();
    let minted: Vec<(u64, &str, Arrival)> = lines
        .iter()
        .map(|line| (line.ordinal, line.digest.as_str(), line.arrived))
        .collect();
    assert_eq!(
        minted,
        vec![
            (1, counted_high_digest.as_str(), Arrival::Recovered),
            (2, counted_low_digest.as_str(), Arrival::Leave),
        ],
        "the reported one is last though its count is lower"
    );
    let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
        .unwrap()
        .expect("a latest");
    assert_eq!(latest.line.digest, counted_low_digest);
}

/// **A FIFO in the room does not wedge the publication** (the custody
/// audit's G1): the member makes a FIFO under a finished name; the
/// publication opens it without blocking, refuses it as no regular file,
/// appends nothing and leaves it in place, answering at once. The scan
/// refuses a FIFO it finds, so a second case has the member make the
/// FIFO after the scan, at a name the scan judged a sound save point:
/// the copy's own open must not block either, and the verb defers. Each
/// runs on a thread with a bound, so a perturbed open fails the test
/// rather than hanging it. Perturbations: open without `O_NONBLOCK` at
/// the scan and the first thread never answers; at the copy, and the
/// second never answers.
#[test]
fn a_fifo_in_the_room_does_not_wedge_the_publication() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-room-fifo-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("published");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = room.join(format!("{}{SUFFIX}", "a".repeat(64)));
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let (room_path, dir_path) = (room.clone(), dir.clone());
    std::thread::spawn(move || {
        let room_fd = open_directory(&room_path).unwrap();
        let dir_fd = open_directory(&dir_path).unwrap();
        let answered = publish(
            room_fd.as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
        )
        .map(|lines| lines.len());
        let _ = tx.send(answered);
    });
    let answered = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("the publication answers, never blocked on the FIFO");
    assert_eq!(answered, Ok(0), "nothing appended");
    assert!(fifo.exists(), "the FIFO is left in place");
    // The FIFO made after the scan, under a name it judged sound.
    let room = base.join("room-swapped");
    let dir = base.join("published-swapped");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = save_point("r-1", 1, 0, 1_000_000_000, b"swapped for a FIFO");
    let swapped = room.join(format!("{}{SUFFIX}", judge(&bytes).unwrap().digest));
    std::fs::write(&swapped, &bytes).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let (room_path, dir_path, fifo) = (room.clone(), dir.clone(), swapped.clone());
    std::thread::spawn(move || {
        let room_fd = open_directory(&room_path).unwrap();
        let dir_fd = open_directory(&dir_path).unwrap();
        let answered = publish_with(
            room_fd.as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
            &mut PublishHooks {
                after_scan: &mut || {
                    std::fs::remove_file(&fifo).unwrap();
                    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600))
                        .unwrap();
                },
                available: &mut available_bytes,
                cap: PUBLISH_CAP,
                room_bound: ROOM_SCAN_BOUND,
                finished: None,
                before_read_back: &mut |_, _| {},
                removal_sync: &mut sync_directory,
                removal: None,
            },
        )
        .map(|(lines, deferred)| (lines.len(), deferred));
        let _ = tx.send(answered);
    });
    let answered = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("the copy answers, never blocked on the FIFO made after the scan");
    assert_eq!(answered, Ok((0, true)), "nothing appended, and deferred");
    assert!(
        swapped.exists(),
        "the FIFO made after the scan is left in place"
    );
}

/// **A listing that fails refuses the selection** (the custody audit's
/// G9): with no manifest, `select` lists the directory to tell a first
/// load from files with no manifest, and a listing that fails is never
/// an empty directory. Perturbation: read the failure as an empty listing
/// and the selection answers a first load.
#[test]
fn a_listing_that_fails_refuses_the_selection() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-list-fails-{}", std::process::id())),
    );
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let mut failing =
        |_: BorrowedFd<'_>| -> nix::Result<Vec<String>> { Err(nix::errno::Errno::EIO) };
    assert!(
        select_with(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            None,
            mine,
            &mut failing
        )
        .is_err()
    );
    assert!(matches!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine),
        Ok(None)
    ));
}

/// **The room opens through the territory without following a link**
/// (the custody audit's G12): a `state` that is a link to a directory
/// holding a save point refuses; a real one opens; an absent one is no
/// room. Perturbation: drop `O_NOFOLLOW` and the link opens.
#[test]
fn the_room_opens_without_following_a_link() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-room-link-{}", std::process::id())),
    );
    let territory = scratch.0.join("territory");
    let elsewhere = scratch.0.join("elsewhere");
    std::fs::create_dir_all(&territory).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    let territory_fd = open_directory(&territory).unwrap();
    assert!(matches!(open_room(territory_fd.as_fd()), Ok(None)));
    std::os::unix::fs::symlink(&elsewhere, territory.join(ROOM)).unwrap();
    assert!(open_room(territory_fd.as_fd()).is_err(), "a link refuses");
    std::fs::remove_file(territory.join(ROOM)).unwrap();
    std::fs::create_dir(territory.join(ROOM)).unwrap();
    assert!(matches!(open_room(territory_fd.as_fd()), Ok(Some(_))));
}

/// A room of `count` finished save points, their clocks ascending, and a
/// published directory beside it; answers the room, the directory, the
/// owner and the room's names and digests oldest first.
fn room_of_save_points(
    tag: &str,
    count: u64,
) -> (
    crate::scratch::Scratch,
    std::path::PathBuf,
    std::path::PathBuf,
    Owner,
    Vec<(String, String)>,
) {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-{tag}-{}", std::process::id())),
    );
    let room = scratch.0.join("room");
    let dir = scratch.0.join("published");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let mut names = Vec::new();
    for sequence in 1..=count {
        let bytes = save_point("r-1", sequence, 0, sequence * 1_000_000_000, b"room");
        let judged = judge(&bytes).unwrap();
        let name = format!("{}{SUFFIX}", judged.digest);
        std::fs::write(room.join(&name), &bytes).unwrap();
        names.push((name, judged.digest));
    }
    (scratch, room, dir, mine, names)
}

/// **A verb publishes at most its cap, oldest first, and stops** (the
/// custody audit's G23): a room of three under a cap of two publishes the
/// two oldest and leaves the newest for the next verb, which publishes
/// it; the newest is never published before an older one, so the
/// highest ordinal stays the last taken. Perturbation: drop the cap and
/// all three publish at once.
#[test]
fn a_verb_publishes_at_most_its_cap_oldest_first() {
    let (_scratch, room, dir, mine, names) = room_of_save_points("room-cap", 3);
    let dir_fd = open_directory(&dir).unwrap();
    let publish_capped = |cap| {
        publish_with(
            open_directory(&room).unwrap().as_fd(),
            mine.uid,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
            &mut PublishHooks {
                after_scan: &mut || {},
                available: &mut available_bytes,
                cap,
                room_bound: ROOM_SCAN_BOUND,
                finished: None,
                before_read_back: &mut |_, _| {},
                removal_sync: &mut sync_directory,
                removal: None,
            },
        )
        .unwrap()
    };
    let (first, deferred) = publish_capped(2);
    let first: Vec<String> = first.into_iter().map(|l| l.digest).collect();
    assert_eq!(first, [names[0].1.clone(), names[1].1.clone()]);
    assert!(deferred, "the verb says it left one");
    assert!(room.join(&names[2].0).exists(), "the newest waits");
    let (second, deferred) = publish_capped(2);
    let second: Vec<String> = second.into_iter().map(|l| l.digest).collect();
    assert_eq!(second, [names[2].1.clone()], "and goes next, highest");
    assert!(!deferred, "the room drained");
}

/// **The scan reads stamps, and the whole reads stop at the cap** (Codex
/// on #94, at ab8acef, and the custody review): a room of forty finished
/// files under the cap of 32 reads each one's stamp line at the scan and
/// reads whole only the 32 it copies, so the time a member's files cost a
/// verb is bounded by the cap and not by the room. Counted through a
/// test-only tally of whole reads. Perturbation: judge each file whole at
/// the scan again and the tally passes the cap.
#[test]
fn the_whole_reads_stop_at_the_cap() {
    let (_scratch, room, dir, mine, _) = room_of_save_points("room-forty", 40);
    let dir_fd = open_directory(&dir).unwrap();
    WHOLE_READS.with(|count| count.set(0));
    let (lines, deferred) = publish_noting_deferral(
        open_directory(&room).unwrap().as_fd(),
        mine.uid,
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        mine,
        &[],
    )
    .unwrap();
    assert_eq!(lines.len(), PUBLISH_CAP);
    assert!(deferred);
    let whole = WHOLE_READS.with(|count| count.get());
    assert!(
        whole <= PUBLISH_CAP,
        "{whole} whole reads for a cap of {PUBLISH_CAP}"
    );
}

/// **A copy is made only where the space stands free, and the first
/// file that does not fit stops the verb** (the custody audit's G23):
/// the older of two room files is the larger, and the filesystem reads
/// exactly enough free for the newer and the reserve, so the older does
/// not fit and the newer would; nothing is copied, the verb says it
/// deferred, and both stay for the next verb, since the newer published
/// past the older would let the older be minted above it later.
/// Perturbations: drop the free-space look and both publish; skip the
/// file that does not fit instead of stopping and the newer publishes.
#[test]
fn a_copy_is_made_only_where_the_space_stands_free() {
    let (_scratch, room, dir, mine, _) = room_of_save_points("room-space", 0);
    let older = save_point("r-1", 1, 0, 1_000_000_000, &[b'o'; 4096]);
    let newer = save_point("r-1", 2, 0, 2_000_000_000, b"newer, smaller");
    assert!(older.len() > newer.len());
    let mut names = Vec::new();
    for bytes in [&older, &newer] {
        let name = format!("{}{SUFFIX}", judge(bytes).unwrap().digest);
        std::fs::write(room.join(&name), bytes).unwrap();
        names.push(name);
    }
    let dir_fd = open_directory(&dir).unwrap();
    let free = newer.len() as u64 + PUBLISH_RESERVE;
    let (lines, deferred) = publish_with(
        open_directory(&room).unwrap().as_fd(),
        mine.uid,
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        mine,
        &[],
        &mut PublishHooks {
            after_scan: &mut || {},
            available: &mut |_| Ok(free),
            cap: PUBLISH_CAP,
            room_bound: ROOM_SCAN_BOUND,
            finished: None,
            before_read_back: &mut |_, _| {},
            removal_sync: &mut sync_directory,
            removal: None,
        },
    )
    .unwrap();
    assert!(
        lines.is_empty(),
        "nothing copied, the newer not past the older"
    );
    assert!(deferred, "and the verb says so");
    assert!(names.iter().all(|name| room.join(name).exists()));
}

/// **A standing line whose target differs stops the publication** (Codex
/// on #94 at 7a25db2): a crash between the line's append and the room
/// copy's removal leaves the copy, and the target is later damaged; the
/// room's copy may be the newest sound state, so the verb defers and
/// the copy stays, and every later save point is left with it: a newer
/// room file taken after it stays in the room and no line is appended.
/// Perturbations: skip the entry, the deferral dropped, and nothing is
/// deferred; skip it with the deferral kept and the newer file is
/// published past it.
#[test]
fn a_standing_line_whose_target_differs_defers() {
    let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-standing-differs-{}",
        std::process::id()
    )));
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("published");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let bytes = save_point("r-1", 4, 1, 1_000_000_000, b"newest");
    let judged = judge(&bytes).unwrap();
    let room_name = format!("{}{SUFFIX}", judged.digest);
    std::fs::write(room.join(&room_name), &bytes).unwrap();
    let publish_now = || {
        publish_noting_deferral(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
        )
        .unwrap()
    };
    let (lines, _) = publish_now();
    assert_eq!(lines.len(), 1);
    // The crash left the room's copy, and the target is damaged; a newer
    // save point, taken after it, waits in the room beside it.
    std::fs::write(room.join(&room_name), &bytes).unwrap();
    std::fs::write(
        dir.join(&lines[0].name),
        save_point("r-1", 4, 1, 1_000_000_000, b"damaged"),
    )
    .unwrap();
    let later = save_point("r-1", 5, 1, 2_000_000_000, b"taken after the standing one");
    let later_name = format!("{}{SUFFIX}", judge(&later).unwrap().digest);
    std::fs::write(room.join(&later_name), &later).unwrap();
    let (appended, deferred) = publish_now();
    assert!(appended.is_empty(), "nothing published past it");
    assert!(deferred, "the verb defers, so a load refuses");
    assert!(room.join(&room_name).exists(), "the room's copy stays");
    assert!(room.join(&later_name).exists(), "the newer file is left");
    assert_eq!(
        read_manifest(dir_fd.as_fd(), mine).unwrap().len(),
        1,
        "no line appended"
    );
}

/// **Recovered save points order by the member process's own count, and
/// those of two processes refuse** (the operator's ruling of 2026-10-08
/// on #99, N3): one process's two files, the second taken after a
/// restore's first distillate landed so that its covered run changed
/// and its sequence fell, and the clock stepped back between them,
/// publish in count order, the later the latest; files of two member
/// processes, or two of one count, refuse and stay in the room. A
/// reported file counts as any other: a recovered file of one process
/// beside a reported one of another refuses too, the process set being
/// built from every entry. Perturbations: order by the covered sequence
/// and the earlier is minted last; key the refusal on the covered run
/// and the first room refuses; drop the process refusal and the second
/// room publishes; build the process set from the recovered files alone
/// and the reported room publishes.
#[test]
fn recovered_save_points_order_by_the_members_count_and_two_processes_refuse() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-count-order-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let room_of = |tag: &str, files: &[Vec<u8>]| {
        let room = base.join(format!("room-{tag}"));
        let dir = base.join(format!("published-{tag}"));
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let mut digests = Vec::new();
        for bytes in files {
            let digest = judge(bytes).unwrap().digest;
            std::fs::write(room.join(format!("{digest}{SUFFIX}")), bytes).unwrap();
            digests.push(digest);
        }
        (room, dir, digests)
    };
    let publish_room = |room: &std::path::Path, dir_fd: BorrowedFd<'_>| {
        publish(
            open_directory(room).unwrap().as_fd(),
            me,
            dir_fd,
            (mine.uid, mine.gid),
            mine,
            &[],
        )
    };
    // Taken first: the restored prior run's position, a late clock.
    let earlier = save_point_taken("r-old", 900, 4, 9_000_000_000, b"earlier", 77, 0);
    // Taken second: the new run's first position, an early clock.
    let later = save_point_taken("r-new", 3, 1, 1_000_000_000, b"later", 77, 1);
    let (room, dir, digests) = room_of("one-process", &[earlier, later]);
    let dir_fd = open_directory(&dir).unwrap();
    let lines = publish_room(&room, dir_fd.as_fd()).expect("one process publishes");
    let minted: Vec<&str> = lines.iter().map(|line| line.digest.as_str()).collect();
    assert_eq!(minted, [digests[0].as_str(), digests[1].as_str()]);
    let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
        .unwrap()
        .expect("a latest");
    assert_eq!(
        latest.line.digest, digests[1],
        "the later taken is the latest"
    );
    // Two member processes: nothing orders them.
    let one = save_point_taken("r-1", 5, 1, 1_000_000_000, b"process one", 77, 0);
    let two = save_point_taken("r-1", 6, 1, 2_000_000_000, b"process two", 78, 0);
    let (room, dir, digests) = room_of("two-processes", &[one, two]);
    let dir_fd = open_directory(&dir).unwrap();
    assert_eq!(
        publish_room(&room, dir_fd.as_fd()).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
    for digest in &digests {
        assert!(room.join(format!("{digest}{SUFFIX}")).exists());
    }
    // A recovered file of one process and a reported file of another:
    // the report does not exempt its file from the process count.
    let recovered = save_point_taken("r-1", 5, 1, 1_000_000_000, b"recovered, 77", 77, 0);
    let reported = save_point_taken("r-1", 6, 1, 2_000_000_000, b"reported, 78", 78, 0);
    let (room, dir, digests) = room_of("reported-other-process", &[recovered, reported]);
    let dir_fd = open_directory(&dir).unwrap();
    let report = weaver_types::SavePointReport {
        save_point: digests[1].clone(),
        name: format!("{}{SUFFIX}", digests[1]),
        run: weaver_types::RunId("r-1".into()),
        sequence: 6,
        turn: 1,
        event_run: weaver_types::RunId("r-1".into()),
        position: 6,
    };
    assert_eq!(
        publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[(report, Arrival::Leave)],
        )
        .err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a reported file of another process refuses"
    );
    for digest in &digests {
        assert!(room.join(format!("{digest}{SUFFIX}")).exists());
    }
    // One process, one count twice: a reused pid, unorderable.
    let first = save_point_taken("r-1", 5, 1, 1_000_000_000, b"count once", 77, 4);
    let again = save_point_taken("r-1", 6, 1, 2_000_000_000, b"count again", 77, 4);
    let (room, dir, _) = room_of("one-count-twice", &[first, again]);
    let dir_fd = open_directory(&dir).unwrap();
    assert_eq!(
        publish_room(&room, dir_fd.as_fd()).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
}

/// **The scan and the copy agree on what a room file is** (the #99 area
/// 1 review, K2): an uppercase-hex name is no finished name, ignored at
/// the scan and never a deferral; a sound file another uid owns, and a
/// save-point format this admin does not read, refuse rather than being
/// passed over while an older save point loads. Perturbations: admit
/// uppercase hex and the first room defers; leave another uid's file in
/// place and the second publishes nothing silently; drop the format look
/// and the third is passed over.
#[test]
fn the_scan_ignores_no_finished_name_and_refuses_what_may_be_newer() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-scan-verdicts-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let room_with = |tag: &str, name: &str, bytes: &[u8]| {
        let room = base.join(format!("room-{tag}"));
        let dir = base.join(format!("published-{tag}"));
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(room.join(name), bytes).unwrap();
        (room, dir)
    };
    let publish_as = |room: &std::path::Path, dir: &std::path::Path, member: u32| {
        let dir_fd = open_directory(dir).unwrap();
        publish_noting_deferral(
            open_directory(room).unwrap().as_fd(),
            member,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
        )
    };
    let bytes = save_point("r-1", 1, 0, 1_000_000_000, b"sound");
    let digest = judge(&bytes).unwrap().digest;
    // Uppercase: no finished name, so nothing is published or deferred.
    let upper = format!("{}{SUFFIX}", digest.to_uppercase());
    let (room, dir) = room_with("upper", &upper, &bytes);
    let (lines, deferred) = publish_as(&room, &dir, me).unwrap();
    assert!(lines.is_empty() && !deferred, "ignored, never deferred");
    assert!(room.join(&upper).exists());
    // Another uid's sound file: may be newer state, so it refuses.
    let lower = format!("{digest}{SUFFIX}");
    let (room, dir) = room_with("other-uid", &lower, &bytes);
    assert_eq!(
        publish_as(&room, &dir, me.wrapping_add(1)).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
    // A format this admin does not read: refuses.
    let newer = String::from_utf8(bytes.clone()).unwrap().replacen(
        "\"weaver-save-point\":1",
        "\"weaver-save-point\":2",
        1,
    );
    assert!(newer.contains("\"weaver-save-point\":2"));
    let (room, dir) = room_with("newer-format", &lower, newer.as_bytes());
    assert_eq!(
        publish_as(&room, &dir, me).err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
}

/// **The save point's bound is one gibibyte in both readers** (the
/// operator's ruling of 2026-10-08 on #1): the member's own constant is
/// pinned to the same number in weaver-state, so the two are held equal
/// by naming it. Perturbation: change either constant.
#[test]
fn the_save_point_bound_is_one_gibibyte() {
    assert_eq!(SAVE_POINT_BOUND, 1_073_741_824);
}

/// **A room file past the bound refuses the publication** (the #94
/// survey's S4): the member never writes one, so one standing is a fault
/// to clear, and left out of the order it would sit in the room while a
/// load restored older state. The file is sparse, so the test writes no
/// gibibyte. Perturbation: leave it in place and answer no entry again,
/// and the publication reads.
#[test]
fn a_room_file_past_the_bound_refuses_the_publication() {
    let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-room-past-bound-{}",
        std::process::id()
    )));
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("published");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let large = std::fs::File::create(room.join(format!("{}{SUFFIX}", "a".repeat(64)))).unwrap();
    large.set_len(SAVE_POINT_BOUND + 1).unwrap();
    drop(large);
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let dir_fd = open_directory(&dir).unwrap();
    assert_eq!(
        publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
        )
        .err(),
        Some(LifecycleRefusal::BoundaryUnverified)
    );
}

/// **The room is copied one file at a time, from bytes judged in the same
/// read** (Codex on #94): each file is read and judged again at its own
/// copy, the scan having kept none of its bytes (`RoomEntry` holds a
/// name and a judgment, so it cannot). The oldest of three, replaced by
/// the member between the scan and its copy, its name kept and its
/// bytes another save point's, is refused at the re-judgment, and the
/// verb stops there: nothing is published, the verb defers, and all
/// three stay in the room. Two layers refuse it, deliberately: the
/// re-judgment's name check in `judge_room_file`, and `publish_with`'s
/// comparison of the judged digest with the scan's. The scan's digest is
/// the name's, so while the name check stands the comparison cannot fire
/// and no room reaches it alone; it is a defence kept beside the check.
/// The name check is pinned on its own by judging the replaced file
/// directly. Perturbations: drop the name check and the direct judgment
/// answers the replaced bytes; drop both and the replaced bytes publish
/// under the name the scan judged.
#[test]
fn the_room_is_copied_from_bytes_judged_in_the_same_read() {
    let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-admin-room-one-at-a-time-{}",
        std::process::id()
    )));
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("published");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let mut names = Vec::new();
    for (sequence, wall) in [
        (1u64, 1_000_000_000u64),
        (2, 2_000_000_000),
        (3, 3_000_000_000),
    ] {
        let bytes = save_point("r-1", sequence, 0, wall, b"room");
        let judged = judge(&bytes).unwrap();
        let name = format!("{}{SUFFIX}", judged.digest);
        std::fs::write(room.join(&name), &bytes).unwrap();
        names.push((name, judged.digest));
    }
    // Between the scan and the copies the member replaces the first file
    // with another save point's bytes under the same name.
    // The newest is the one the harness reported.
    let swapped = room.join(&names[0].0);
    let other = save_point("r-1", 9, 0, 9_000_000_000, b"other");
    let report = weaver_types::SavePointReport {
        save_point: names[2].1.clone(),
        name: names[2].0.clone(),
        run: weaver_types::RunId("r-1".into()),
        sequence: 3,
        turn: 0,
        event_run: weaver_types::RunId("r-1".into()),
        position: 7,
    };
    let (lines, deferred) = publish_with(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        mine,
        &[(report, Arrival::Demand)],
        &mut PublishHooks {
            after_scan: &mut || std::fs::write(&swapped, &other).unwrap(),
            available: &mut available_bytes,
            cap: PUBLISH_CAP,
            room_bound: ROOM_SCAN_BOUND,
            finished: None,
            before_read_back: &mut |_, _| {},
            removal_sync: &mut sync_directory,
            removal: None,
        },
    )
    .unwrap();
    // **The oldest failing its judgment stops the verb** (the #94
    // survey's S3): published past, it would leave the oldest to be
    // minted later above the reported one. Perturbation: skip the
    // failed entry again and the two newer publish.
    assert!(lines.is_empty(), "nothing newer is published past it");
    assert!(deferred, "and the verb answers the deferral");
    for (name, _) in &names {
        assert!(room.join(name).exists(), "{name} is left in the room");
    }
    // The name check on its own: the replaced file, judged directly, is
    // no save point under its name, and an untouched one judges sound.
    let room_fd = open_directory(&room).unwrap();
    assert!(
        judge_room_file(room_fd.as_fd(), &names[0].0, me).is_none(),
        "bytes that are not the name's are refused at the re-judgment"
    );
    assert_eq!(
        judge_room_file(room_fd.as_fd(), &names[1].0, me).map(|(_, judged)| judged.digest),
        Some(names[1].1.clone())
    );
}

/// **A publication interrupted after the rename is adopted at the next
/// verb**, per `weaver-admin-Spec` section 6's retry: the target stands
/// under its published name with no manifest line, and the next
/// publication judges it as this save point, appends the line and removes
/// the room's copy; an entry of other bytes under that name refuses and
/// the room's copy stays. Perturbation: treat `EEXIST` as a failure again
/// and the first case never gets its line.
#[test]
fn an_interrupted_publication_is_adopted_and_an_impostor_is_not() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-adopt-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("decl");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let owner = (me, mine.gid);
    let bytes = save_point("r-1", 7, 1, 5_000_000_000, b"interrupted");
    let judged = judge(&bytes).unwrap();
    let room_name = format!("{}{SUFFIX}", judged.digest);
    std::fs::write(room.join(&room_name), &bytes).unwrap();
    // The target stands already, as the interrupted rename left it.
    let target = dir.join(published_name(&judged));
    std::fs::write(&target, &bytes).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[],
    )
    .unwrap();
    assert_eq!(
        lines.len(),
        1,
        "the line is appended for the adopted target"
    );
    assert_eq!(lines[0].digest, judged.digest);
    assert!(!room.join(&room_name).exists(), "the room's copy went");
    // An impostor under the published name: other bytes, the room's
    // copy stays and no line is appended.
    let other = save_point("r-1", 8, 1, 6_000_000_000, b"other");
    let other_judged = judge(&other).unwrap();
    std::fs::write(
        room.join(format!("{}{SUFFIX}", other_judged.digest)),
        &other,
    )
    .unwrap();
    let impostor = dir.join(published_name(&other_judged));
    std::fs::write(&impostor, b"not those bytes").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&impostor, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[],
    )
    .unwrap();
    assert!(lines.is_empty(), "no line for the impostor's name");
    assert!(
        room.join(format!("{}{SUFFIX}", other_judged.digest))
            .exists(),
        "the room's copy stays for the next verb"
    );
    // **A line standing for the room's copy is judged before the copy
    // goes** (Codex on #94, round 9): the first save point's line stands;
    // its room copy put back and its target removed, the publication
    // recreates the target under the standing line, appends nothing and
    // removes the copy; the copy put back and the target replaced by
    // other bytes, the copy stays and no line is appended. Perturbation:
    // remove the copy on the digest alone and the first case loses the
    // last sound copy with the target still gone.
    std::fs::write(room.join(&room_name), &bytes).unwrap();
    std::fs::remove_file(&target).unwrap();
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[],
    )
    .unwrap();
    assert!(lines.is_empty(), "no second line for a standing one");
    assert!(target.exists(), "the target is recreated under its line");
    assert!(
        !room.join(&room_name).exists(),
        "the copy went once the target stood"
    );
    assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 1);
    std::fs::write(room.join(&room_name), &bytes).unwrap();
    std::fs::write(&target, b"damaged").unwrap();
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[],
    )
    .unwrap();
    assert!(lines.is_empty());
    assert!(room.join(&room_name).exists(), "the last sound copy stays");
    std::fs::write(&target, &bytes).unwrap();
}

/// **The corpus holds the two readers equal**, per `weaver-admin-Spec`
/// section 4: every file of the workspace's save-point corpus judges to
/// the verdict the corpus gives it, so a case this reader admits and the
/// member's refuses, or the reverse, fails here or in the member's own
/// corpus test. Perturbation: relax any one rule of `judge` and its case
/// judges sound against a `refuses` verdict.
#[test]
fn the_corpus_holds_the_judgment_equal_to_the_members_parse() {
    let corpus =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../weaver-types/tests/fixtures/save-points");
    let verdicts = std::fs::read_to_string(corpus.join("verdicts.txt")).expect("the verdicts");
    let mut cases = 0;
    for line in verdicts.lines().filter(|l| !l.trim().is_empty()) {
        let (name, verdict) = line.split_once(' ').expect("name and verdict");
        let bytes = std::fs::read(corpus.join(name)).expect(name);
        let judged = judge(&bytes);
        match verdict {
            "sound" => assert!(judged.is_ok(), "{name} judges sound: {judged:?}"),
            "refuses" => assert!(judged.is_err(), "{name} refuses"),
            other => panic!("{name}: verdict {other} is not sound or refuses"),
        }
        cases += 1;
    }
    assert!(cases >= 10, "the corpus holds its cases: {cases}");
}

/// **The civil date is the epoch's own**: known instants render as the
/// UTC stamps the name carries.
#[test]
fn the_utc_stamp_renders_known_instants() {
    assert_eq!(utc_stamp(0), "19700101T000000Z");
    assert_eq!(utc_stamp(1_759_788_000), "20251006T220000Z");
    assert_eq!(utc_stamp(951_782_400), "20000229T000000Z");
}

/// **The manifest's lines round-trip and the latest is read from it**,
/// per `weaver-admin-Spec` sections 4 and 6: a latest whose file is
/// gone, or whose bytes do not digest to its line, refuses the load and
/// an older one loads only by name, the ordinal is one past the highest line standing
/// even when that line's file is gone, a file no line names is not
/// loadable by name, and a manifest that does not parse refuses
/// `BoundaryUnverified`. The checks behind "differs" are pinned each on
/// its own at the end: a line naming a sound file by its published name
/// and carrying another's digest refuses, and a manifest of ordinals 1
/// and 5 mints 6. Perturbations: drop the digest comparison in
/// `open_published` and the line of another's digest selects its file;
/// mint the ordinal from the count of lines and the gapped manifest
/// mints 3.
#[test]
fn the_latest_is_read_from_the_manifest_and_a_file_it_does_not_name_is_not_loadable() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-manifest-{}", std::process::id())),
    );
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let mut lines = Vec::new();
    for (sequence, wall) in [
        (1u64, 1_000_000_000u64),
        (2, 2_000_000_000),
        (3, 3_000_000_000),
    ] {
        let bytes = save_point("r-1", sequence, 0, wall, b"img");
        let judged = judge(&bytes).unwrap();
        let name = published_name(&judged);
        std::fs::write(dir.join(&name), &bytes).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o640)).unwrap();
        let line = ManifestLine {
            ordinal: next_ordinal(&lines),
            digest: judged.digest,
            name,
            stamp: judged.stamp,
            position: Some(("r-1".into(), sequence + 10)),
            arrived: if sequence == 2 {
                Arrival::Restore
            } else {
                Arrival::Leave
            },
        };
        append_line(dir_fd.as_fd(), mine, &line).unwrap();
        lines.push(line);
    }
    assert_eq!(
        read_manifest(dir_fd.as_fd(), mine).unwrap(),
        lines,
        "the lines round-trip"
    );
    let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
        .unwrap()
        .expect("a latest");
    assert_eq!(latest.line.ordinal, 3);
    assert!(!latest.lineage.named_at_restore);
    // **A latest that cannot be judged refuses, never passed over** (the
    // #94 survey's S9): its mode changed by hand to 0644 is a fault, not
    // a file gone or differing, so the selection refuses rather than
    // restore the second. Perturbation: pass over every fault again and
    // the second is selected.
    {
        use std::os::unix::fs::PermissionsExt;
        let third = dir.join(&lines[2].name);
        std::fs::set_permissions(&third, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
            Some(LifecycleRefusal::BoundaryUnverified)
        );
        std::fs::set_permissions(&third, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    // **The latest gone or differing refuses, never passed over** (the
    // Planner's ruling of 2026-10-08 on #94): the third standing with
    // other sound bytes refuses, and gone it refuses, the next ordinal
    // still four. **A `[restore]` naming an older line refuses too**
    // (the operator's ruling of 2026-10-08 on #99, N6), naming both, and
    // the `restore` verb is what makes the named one the latest: it is
    // named again under the next ordinal, and the load then continues
    // from it, by the key or by default. Perturbations: pass over to an
    // older line again and the first two refusals answer; honour a
    // `[restore]` that is not the latest and the third answers; append
    // nothing for a listed name and the fourth refuses.
    let third = dir.join(&lines[2].name);
    std::fs::write(
        &third,
        save_point("r-1", 3, 0, 3_000_000_000, b"other image"),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&third, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    assert_eq!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a latest that differs refuses"
    );
    std::fs::remove_file(&third).unwrap();
    assert_eq!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a latest gone refuses"
    );
    assert_eq!(
        next_ordinal(&read_manifest(dir_fd.as_fd(), mine).unwrap()),
        4
    );
    assert!(
        matches!(
            select(
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                Some(&lines[1].name),
                mine,
            ),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ),
        "a [restore] naming an older line than the latest refuses"
    );
    let again =
        name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &lines[1].name, mine).unwrap();
    assert_eq!(
        (again.ordinal, again.arrived, again.digest.as_str()),
        (4, Arrival::Restore, lines[1].digest.as_str()),
        "the restore verb names the older one again as the latest"
    );
    for restore in [None, Some(lines[1].name.as_str())] {
        let named = select(dir_fd.as_fd(), (mine.uid, mine.gid), restore, mine)
            .unwrap()
            .expect("the renamed one");
        assert_eq!(named.line.ordinal, 4);
        assert!(named.lineage.named_at_restore);
    }
    let first =
        name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &lines[0].digest, mine).unwrap();
    assert_eq!(first.ordinal, 5);
    let named = select(
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        Some(&lines[0].digest),
        mine,
    )
    .unwrap()
    .expect("the named one");
    assert_eq!(named.line.ordinal, 5);
    // A file no line names is not loadable by name.
    let stray = save_point("r-9", 9, 0, 9_000_000_000, b"stray");
    let stray_name = published_name(&judge(&stray).unwrap());
    std::fs::write(dir.join(&stray_name), &stray).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join(&stray_name),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
    }
    assert!(matches!(
        select(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            Some(&stray_name),
            mine
        ),
        Err(LifecycleRefusal::ConfigInvalid { .. })
    ));
    // Named at a restore by its bare digest, the unlisted file is found
    // under its published name (Codex on #94, round 2), enters with the
    // next ordinal and is then loadable by name and by digest; a digest
    // no file carries refuses. Perturbation: open the digest as a name
    // and the first call refuses.
    let stray_digest = judge(&stray).unwrap().digest;
    assert!(name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &"0".repeat(64), mine).is_err());
    let named = name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_digest, mine).unwrap();
    assert_eq!(named.name, stray_name);
    assert_eq!((named.ordinal, named.arrived), (6, Arrival::Restore));
    let by_name = select(
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        Some(&stray_name),
        mine,
    )
    .unwrap()
    .unwrap();
    assert_eq!(by_name.line.ordinal, 6);
    let by_digest = select(
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        Some(&named.digest),
        mine,
    )
    .unwrap()
    .unwrap();
    assert!(by_digest.lineage.named_at_restore);
    // **A listed name is judged at the verb** (Codex on #94, round 4):
    // the file holding other sound bytes refuses as the load would, gone
    // it refuses the boundary, and standing again it answers its line.
    // Perturbation: answer the listed line unjudged and the first two
    // calls answer.
    std::fs::write(
        dir.join(&stray_name),
        save_point("r-9", 9, 0, 9_000_000_001, b"swapped"),
    )
    .unwrap();
    assert!(matches!(
        name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_name, mine),
        Err(LifecycleRefusal::ConfigInvalid { .. })
    ));
    std::fs::rename(dir.join(&stray_name), dir.join("aside")).unwrap();
    assert!(matches!(
        name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_digest, mine),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    std::fs::rename(dir.join("aside"), dir.join(&stray_name)).unwrap();
    std::fs::write(dir.join(&stray_name), &stray).unwrap();
    assert_eq!(
        name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_name, mine)
            .unwrap()
            .ordinal,
        6
    );
    // A renamed file does not enter.
    std::fs::rename(
        dir.join(&stray_name),
        dir.join("20200101T000000Z-aa.save-point"),
    )
    .unwrap();
    assert!(
        name_at_restore(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            "20200101T000000Z-aa.save-point",
            mine
        )
        .is_err()
    );
    // Back under its name, the latest stands again for what follows.
    std::fs::rename(
        dir.join("20200101T000000Z-aa.save-point"),
        dir.join(&stray_name),
    )
    .unwrap();
    // **The manifest is judged as the owner's**: against another expected
    // owner the one that stands reads as the operator's and refuses, read
    // and appended, its planted lines selecting nothing; a second link
    // refuses by count; a symbolic link refuses as a link; the owner's
    // own reads. Perturbation: drop the uid comparison and the first
    // case reads, its lines selecting the files they name.
    let other = Owner {
        uid: me.wrapping_add(1),
        gid: mine.gid,
    };
    assert!(matches!(
        read_manifest(dir_fd.as_fd(), other),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    assert!(matches!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, other),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    assert!(matches!(
        append_line(dir_fd.as_fd(), other, &lines[0]),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    std::fs::hard_link(dir.join(MANIFEST), dir.join("manifest-link")).unwrap();
    assert!(matches!(
        read_manifest(dir_fd.as_fd(), mine),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    std::fs::remove_file(dir.join("manifest-link")).unwrap();
    assert!(
        read_manifest(dir_fd.as_fd(), mine).is_ok(),
        "the owner's own reads again"
    );
    std::fs::rename(dir.join(MANIFEST), dir.join("manifest-real")).unwrap();
    std::os::unix::fs::symlink(dir.join("manifest-real"), dir.join(MANIFEST)).unwrap();
    assert!(matches!(
        read_manifest(dir_fd.as_fd(), mine),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    std::fs::remove_file(dir.join(MANIFEST)).unwrap();
    std::fs::rename(dir.join("manifest-real"), dir.join(MANIFEST)).unwrap();
    // **A torn last line is dropped and the next append truncates it**
    // (Codex on #94, round 3): the lines before it read, the latest is
    // selected among them, and after an append the file holds whole
    // lines alone. Perturbation: parse the torn line and the read
    // refuses the whole manifest.
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(MANIFEST))
            .unwrap();
        file.write_all(b"{\"ordinal\":9,\"dig").unwrap();
    }
    let read = read_manifest(dir_fd.as_fd(), mine).unwrap();
    assert_eq!(read.len(), 6, "the torn line is dropped: {read:?}");
    assert!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .is_some()
    );
    append_line(dir_fd.as_fd(), mine, &lines[0]).unwrap();
    let text = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
    assert!(
        !text.contains("\"dig\n") && !text.contains("\"dig{"),
        "{text}"
    );
    assert!(text.ends_with('\n'));
    assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 7);
    // A manifest that does not parse refuses, and so does one absent
    // beside published files; absent beside none is the first load.
    std::fs::write(dir.join(MANIFEST), "not json\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(MANIFEST), std::fs::Permissions::from_mode(0o644))
            .unwrap();
    }
    assert!(matches!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    std::fs::remove_file(dir.join(MANIFEST)).unwrap();
    assert!(matches!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    // A manifest of another mode is not this crate's and refuses, read
    // or appended.
    std::fs::write(dir.join(MANIFEST), "").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(MANIFEST), std::fs::Permissions::from_mode(0o666))
            .unwrap();
    }
    assert!(matches!(
        read_manifest(dir_fd.as_fd(), mine),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    assert!(matches!(
        append_line(dir_fd.as_fd(), mine, &lines[0]),
        Err(LifecycleRefusal::BoundaryUnverified)
    ));
    std::fs::remove_file(dir.join(MANIFEST)).unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        std::fs::remove_file(entry.path()).unwrap();
    }
    assert!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .is_none(),
        "nothing at all is the first load"
    );
    // **A line whose digest is not its file's refuses** (the #99 area 1
    // review), apart from the name check: the line names a sound file
    // by the published name its bytes compute, so `open_judged` passes
    // it, and carries another save point's digest.
    let sound = save_point("r-1", 1, 0, 1_000_000_000, b"sound");
    let sound_judged = judge(&sound).unwrap();
    let sound_name = published_name(&sound_judged);
    std::fs::write(dir.join(&sound_name), &sound).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join(&sound_name),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();
    }
    let elsewhere = judge(&save_point("r-1", 2, 0, 2_000_000_000, b"elsewhere"))
        .unwrap()
        .digest;
    append_line(
        dir_fd.as_fd(),
        mine,
        &ManifestLine {
            ordinal: 1,
            digest: elsewhere,
            name: sound_name.clone(),
            stamp: sound_judged.stamp.clone(),
            position: None,
            arrived: Arrival::Leave,
        },
    )
    .unwrap();
    assert_eq!(
        select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
        Some(LifecycleRefusal::BoundaryUnverified),
        "a file of its name and not its line's digest refuses"
    );
    // **The ordinal is one past the highest line, not the count of
    // lines**: lines 1 and 5 stand, and the next publication mints 6.
    std::fs::remove_file(dir.join(MANIFEST)).unwrap();
    for ordinal in [1, 5] {
        append_line(
            dir_fd.as_fd(),
            mine,
            &ManifestLine {
                ordinal,
                ..lines[0].clone()
            },
        )
        .unwrap();
    }
    assert_eq!(
        next_ordinal(&read_manifest(dir_fd.as_fd(), mine).unwrap()),
        6
    );
    let room = dir.join("room");
    std::fs::create_dir(&room).unwrap();
    let next = save_point("r-1", 6, 0, 6_000_000_000, b"next");
    std::fs::write(
        room.join(format!("{}{SUFFIX}", judge(&next).unwrap().digest)),
        &next,
    )
    .unwrap();
    let appended = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        mine,
        &[],
    )
    .unwrap();
    assert_eq!(
        appended.iter().map(|line| line.ordinal).collect::<Vec<_>>(),
        [6],
        "minted one past the highest line"
    );
}

/// **The marker's three states round-trip and resolve the reset**, per
/// `weaver-admin-Spec` section 4: open is `NoCleanUnload`, forced is
/// `ForcedUnload`, closed and absent are none, and writing `None`
/// removes it.
#[test]
fn the_marker_round_trips_and_resolves_the_reset() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-marker-{}", std::process::id())),
    );
    let root = scratch.0.clone();
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(read_marker(&root), None);
    assert_eq!(reset_from(None), None);
    for marker in [
        Marker::Open { run: "r-1".into() },
        Marker::Closed { run: "r-1".into() },
        Marker::Forced { run: "r-2".into() },
    ] {
        write_marker(&root, Some(&marker)).unwrap();
        assert_eq!(read_marker(&root).as_ref(), Some(&marker));
    }
    assert_eq!(
        reset_from(Some(&Marker::Open { run: "r-1".into() })),
        Some(weaver_types::Reset {
            prior_run: weaver_types::RunId("r-1".into()),
            reason: weaver_types::ResetReason::NoCleanUnload,
        })
    );
    assert_eq!(
        reset_from(Some(&Marker::Forced { run: "r-2".into() })).map(|r| r.reason),
        Some(weaver_types::ResetReason::ForcedUnload)
    );
    assert_eq!(
        reset_from(Some(&Marker::Closed { run: "r-1".into() })),
        None
    );
    write_marker(&root, None).unwrap();
    assert_eq!(read_marker(&root), None);
}

/// The process's umask as `/proc/self/status` reports it, read without
/// changing it (a `umask` call both reads and sets).
fn process_umask() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("Umask:"))?;
    u32::from_str_radix(line.split_whitespace().nth(1)?, 8).ok()
}

/// **Runs `body` in a child process** (#113): this test binary re-executed,
/// filtered to the one test `name` names and told by the environment to run
/// the body there. A umask is process-wide, so a test that sets one runs it
/// where no sibling test lives. The parent passes only where the child ran
/// that one test and it passed.
fn in_a_child(name: &str, body: impl FnOnce()) {
    const CHILD: &str = "WEAVER_ADMIN_IN_A_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        body();
        return;
    }
    let path = format!("save_points::tests::{name}");
    let ran = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", &path, "--test-threads", "1"])
        .env(CHILD, name)
        .output()
        .expect("the child runs");
    let said = String::from_utf8_lossy(&ran.stdout);
    assert!(
        ran.status.success() && said.contains("test result: ok. 1 passed"),
        "{name} in its child: {said}{}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// **The umask tests never change this process's umask** (#113): a umask is
/// process-wide, and a sibling test creating directories while one of
/// them ran under 000 left its scratch world-writable, which the boundary
/// checks then refused. A watcher thread reads the umask without changing
/// it while both tests run ten times each; it must never see anything
/// but the umask it started with. Perturbation: run their bodies in this
/// process again, and the watcher sees 000 or 077.
#[test]
fn the_umask_tests_never_leak_their_umask() {
    let Some(start) = process_umask() else {
        eprintln!("SKIP: no /proc/self/status umask to watch");
        return;
    };
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
    let (watching, into) = (done.clone(), seen.clone());
    let watcher = std::thread::spawn(move || {
        while !watching.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(mask) = process_umask() {
                into.lock().unwrap().insert(mask);
            }
            // A short sleep rather than a spin, so the watch does not pin a
            // core beside the suite's timing tests (#113).
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
    });
    for _ in 0..10 {
        the_marker_is_0644_whatever_the_umask();
        the_published_copy_is_0640_whatever_the_umask();
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    watcher.join().expect("the watcher");
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        std::collections::BTreeSet::from([start]),
        "the process umask stayed {start:o}"
    );
}

/// **The marker is 0644 whatever the umask**, per Spec section 4 (Codex
/// on #94, round 10): under a umask of 000 the create alone would leave
/// it 0666 in the root; the mode is set through the descriptor.
/// Perturbation: drop the `fchmod` and the marker reads 0666.
#[test]
fn the_marker_is_0644_whatever_the_umask() {
    in_a_child(
        "the_marker_is_0644_whatever_the_umask",
        marker_mode_under_umask_000,
    );
}

fn marker_mode_under_umask_000() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-marker-umask-{}", std::process::id())),
    );
    std::fs::create_dir_all(&scratch.0).unwrap();
    let before = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
    let written = write_marker(&scratch.0, Some(&Marker::Open { run: "r-1".into() }));
    nix::sys::stat::umask(before);
    written.unwrap();
    let mode = std::fs::metadata(scratch.0.join(MARKER))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o7777, 0o644);
}

/// **The published copy is 0640 whatever the umask**, per Spec section 6
/// (Codex on #94, round 8): under a umask of 077 the open alone would
/// make it 0600 and the exact-mode check would refuse every publication;
/// the mode is set through the descriptor. Perturbation: drop the
/// `fchmod` and nothing publishes under that umask.
#[test]
fn the_published_copy_is_0640_whatever_the_umask() {
    in_a_child(
        "the_published_copy_is_0640_whatever_the_umask",
        published_copy_mode_under_umask_077,
    );
}

fn published_copy_mode_under_umask_077() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-umask-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("decl");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let owner = (me, mine.gid);
    let bytes = save_point("r-1", 7, 1, 5_000_000_000, b"under a umask");
    let judged = judge(&bytes).unwrap();
    std::fs::write(room.join(format!("{}{SUFFIX}", judged.digest)), &bytes).unwrap();
    let before = nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
    let published = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[],
    );
    nix::sys::stat::umask(before);
    let lines = published.unwrap();
    assert_eq!(lines.len(), 1, "the copy published under the umask");
    let mode = std::fs::metadata(dir.join(&lines[0].name))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o7777, 0o640);
}

/// **The publication lands in the judged directory whatever the path
/// does**, per `weaver-admin-Spec` section 9 (Codex on #94, round 7):
/// the directory is opened once, then renamed away and another put at
/// its path; the publication, its manifest line and the selection all go
/// through the descriptor and land in the directory judged, the one at
/// the path getting nothing. The pin is the type: every step takes the
/// descriptor and no path reaches it, so the perturbation is the watch's
/// own, opening the descriptor after the swap, which puts everything in
/// the replacement and fails the assertion.
#[test]
fn the_publication_lands_in_the_judged_directory_whatever_the_path_does() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-swap-{}", std::process::id())),
    );
    let base = scratch.0.clone();
    let room = base.join("room");
    let dir = base.join("decl");
    std::fs::create_dir_all(&room).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let me = nix::unistd::getuid().as_raw();
    let mine = Owner {
        uid: me,
        gid: nix::unistd::getgid().as_raw(),
    };
    let owner = (me, mine.gid);
    let bytes = save_point("r-1", 7, 1, 5_000_000_000, b"judged directory");
    let judged = judge(&bytes).unwrap();
    std::fs::write(room.join(format!("{}{SUFFIX}", judged.digest)), &bytes).unwrap();
    // The swap: the judged directory moves aside and a replacement
    // stands at the path.
    let aside = base.join("decl-judged");
    std::fs::rename(&dir, &aside).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let lines = publish(
        open_directory(&room).unwrap().as_fd(),
        me,
        dir_fd.as_fd(),
        owner,
        mine,
        &[],
    )
    .unwrap();
    assert_eq!(lines.len(), 1);
    assert!(
        aside.join(&lines[0].name).exists() && aside.join(MANIFEST).exists(),
        "the target and the line land in the directory judged"
    );
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "the directory at the path gets nothing"
    );
    let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
        .unwrap()
        .expect("a latest");
    assert_eq!(latest.line.digest, judged.digest);
    // A descriptor opened at the path now reaches the replacement, which
    // holds no manifest and no file: the first load's answer.
    let replacement = open_directory(&dir).unwrap();
    assert!(
        select(replacement.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .is_none()
    );
}

/// **A FIFO at the manifest's name is refused, never blocking** (#98; the
/// custody audit's G15, measured no gap): on Linux an `O_RDWR` open of a FIFO
/// never blocks (fifo(7)), so the append's open answers at once, and the
/// manifest's judgment refuses anything not a regular file before a byte is
/// read. Perturbation: drop the regular-file check from `judge_manifest`, and
/// the tail's read blocks on the FIFO past this test's bound.
#[test]
fn a_fifo_at_the_manifest_is_refused_without_blocking() {
    let scratch = crate::scratch::Scratch(
        std::env::temp_dir().join(format!("weaver-admin-manifest-fifo-{}", std::process::id())),
    );
    let dir = scratch.0.clone();
    std::fs::create_dir_all(&dir).unwrap();
    nix::unistd::mkfifo(
        &dir.join(MANIFEST),
        nix::sys::stat::Mode::from_bits_truncate(0o644),
    )
    .unwrap();
    let mine = Owner {
        uid: nix::unistd::getuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
    };
    let line = ManifestLine {
        ordinal: 1,
        digest: "ab".into(),
        name: "ab.save-point".into(),
        stamp: Stamp {
            run: "r-1".into(),
            sequence: 1,
            turn: 0,
            schema: String::new(),
            wall_ns: 0,
        },
        position: None,
        arrived: Arrival::Leave,
    };
    let (sent, answered) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let dir_fd = open_directory(&dir).unwrap();
        let _ = sent.send(append_line(dir_fd.as_fd(), mine, &line));
    });
    assert_eq!(
        answered.recv_timeout(std::time::Duration::from_secs(5)),
        Ok(Err(LifecycleRefusal::BoundaryUnverified)),
        "refused at once"
    );
    drop(scratch);
}

/// **The room's scan holds at most its bound of finished names** (S16, the
/// operator's ruling of 2026-10-14 on #140; Spec section 6): a room at the
/// bound scans and publishes, names that are not finished not counted, and
/// the count comes back for `admin.log`; one finished name past it refuses
/// `BoundaryUnverified` before any is scanned, nothing published, the count
/// past the bound. The bound here is the test's three; production's is
/// pinned beside. Perturbations: drop the refusal and the room past the bound
/// publishes; count every name and the room at the bound refuses.
#[test]
fn the_rooms_scan_refuses_past_its_bound_and_counts_what_it_saw() {
    assert_eq!(ROOM_SCAN_BOUND, 4096, "the bound the Spec states");
    let publish_bounded = |room: &std::path::Path, dir: &std::path::Path, mine: Owner| {
        let mut hooks = PublishHooks {
            after_scan: &mut || {},
            available: &mut available_bytes,
            cap: PUBLISH_CAP,
            room_bound: 3,
            finished: None,
            before_read_back: &mut |_, _| {},
            removal_sync: &mut sync_directory,
            removal: None,
        };
        let published = publish_with(
            open_directory(room).unwrap().as_fd(),
            mine.uid,
            open_directory(dir).unwrap().as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
            &mut hooks,
        );
        (published, hooks.finished)
    };
    // At the bound, beside names that are not finished.
    let (_at, room, dir, mine, names) = room_of_save_points("room-at-bound", 3);
    std::fs::write(room.join("notes.txt"), b"not a save point").unwrap();
    std::fs::write(room.join(format!("{}.part", "a".repeat(64))), b"unfinished").unwrap();
    let (published, finished) = publish_bounded(&room, &dir, mine);
    let (lines, _) = published.expect("a room at the bound scans");
    assert_eq!(lines.len(), names.len());
    assert_eq!(finished, Some(3));
    // One past it.
    let (_past, room, dir, mine, names) = room_of_save_points("room-past-bound", 4);
    let (published, finished) = publish_bounded(&room, &dir, mine);
    assert_eq!(published.err(), Some(LifecycleRefusal::BoundaryUnverified));
    assert_eq!(finished, Some(4), "the count past the bound");
    assert!(
        read_manifest(open_directory(&dir).unwrap().as_fd(), mine)
            .unwrap()
            .is_empty()
    );
    assert!(
        names.iter().all(|(name, _)| room.join(name).exists()),
        "nothing taken from the room"
    );
}

/// **The published copy is read back before its line is written** (#157;
/// Spec section 6): a reported save point whose published copy is
/// corrupted between the rename and the read-back appends no line, so the
/// verb's report did not land (3.0: `SavePointNotTaken` naming
/// `published`); the corrupted copy is removed, the room's copy stays and
/// the verb says it deferred. The next publication, its read-back clean,
/// lands the same save point under its line. Perturbation: drop the
/// read-back and the corrupted copy gets its line; make the read-back
/// refuse every copy and the clean publication lands nothing.
#[test]
fn a_published_copy_that_does_not_read_back_gets_no_line() {
    let (_scratch, room, dir, mine, _) = room_of_save_points("read-back", 0);
    let bytes = save_point("r-1", 3, 1, 3_000_000_000, b"reported");
    let digest = judge(&bytes).unwrap().digest;
    let room_name = format!("{digest}{SUFFIX}");
    std::fs::write(room.join(&room_name), &bytes).unwrap();
    let report = || weaver_types::SavePointReport {
        save_point: digest.clone(),
        name: room_name.clone(),
        run: weaver_types::RunId("r-1".into()),
        sequence: 3,
        turn: 1,
        event_run: weaver_types::RunId("r-1".into()),
        position: 3,
    };
    let dir_fd = open_directory(&dir).unwrap();
    let mut corrupted = None;
    let (lines, deferred) = publish_with(
        open_directory(&room).unwrap().as_fd(),
        mine.uid,
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        mine,
        &[(report(), Arrival::Demand)],
        &mut PublishHooks {
            after_scan: &mut || {},
            available: &mut available_bytes,
            cap: PUBLISH_CAP,
            room_bound: ROOM_SCAN_BOUND,
            finished: None,
            before_read_back: &mut |_, name| {
                // One byte of the payload flipped in place, the file's
                // owner, mode and length as the publication left them.
                let path = dir.join(name);
                let mut published = std::fs::read(&path).unwrap();
                *published.last_mut().unwrap() ^= 0xff;
                std::fs::write(&path, &published).unwrap();
                corrupted = Some(name.to_string());
            },
            removal_sync: &mut sync_directory,
            removal: None,
        },
    )
    .unwrap();
    let corrupted = corrupted.expect("the read-back hook ran");
    assert!(
        lines.is_empty(),
        "a copy that does not read back gets no line: {lines:?}"
    );
    assert!(deferred, "and the verb says it deferred");
    assert!(
        read_manifest(dir_fd.as_fd(), mine).unwrap().is_empty(),
        "the manifest names nothing"
    );
    assert!(
        !dir.join(&corrupted).exists(),
        "the corrupted copy is removed, so the next verb publishes afresh"
    );
    assert!(room.join(&room_name).exists(), "the room's copy stays");

    let (lines, deferred) = publish_noting_deferral(
        open_directory(&room).unwrap().as_fd(),
        mine.uid,
        dir_fd.as_fd(),
        (mine.uid, mine.gid),
        mine,
        &[(report(), Arrival::Demand)],
    )
    .unwrap();
    assert!(!deferred);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.digest.as_str())
            .collect::<Vec<_>>(),
        vec![digest.as_str()],
        "a clean read-back lands the save point under its line"
    );
    assert_eq!(std::fs::read(dir.join(&lines[0].name)).unwrap(), bytes);
    assert!(!room.join(&room_name).exists(), "and the room's copy goes");
}

/// **A target recreated beneath its standing line is read back too** (#157):
/// a line stands, its target is gone and the room's copy is left, so the
/// publication recreates the target under the line; corrupted before its
/// read-back, the recreated copy is removed, the verb defers, the room's
/// copy stays and the manifest still holds its one line, so the next verb
/// recreates it again. Perturbation: drop the read-back and the corrupted
/// copy stands under the line.
#[test]
fn a_recreated_target_that_does_not_read_back_is_removed() {
    let (_scratch, room, dir, mine, names) = room_of_save_points("read-back-recreate", 1);
    let room_name = names[0].0.clone();
    let room_bytes = std::fs::read(room.join(&room_name)).unwrap();
    let dir_fd = open_directory(&dir).unwrap();
    let publish_now = |before_read_back: &mut dyn FnMut(BorrowedFd<'_>, &str)| {
        publish_with(
            open_directory(&room).unwrap().as_fd(),
            mine.uid,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
            &mut PublishHooks {
                after_scan: &mut || {},
                available: &mut available_bytes,
                cap: PUBLISH_CAP,
                room_bound: ROOM_SCAN_BOUND,
                finished: None,
                before_read_back,
                removal_sync: &mut sync_directory,
                removal: None,
            },
        )
        .unwrap()
    };
    let (lines, _) = publish_now(&mut |_, _| {});
    assert_eq!(lines.len(), 1);
    let published = dir.join(&lines[0].name);
    // A crash left the room's copy, and the target is gone since.
    std::fs::write(room.join(&room_name), &room_bytes).unwrap();
    std::fs::remove_file(&published).unwrap();

    let (appended, deferred) = publish_now(&mut |_, name| {
        let path = dir.join(name);
        let mut bytes = std::fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        std::fs::write(&path, &bytes).unwrap();
    });
    assert!(appended.is_empty(), "no second line");
    assert!(deferred, "the verb defers");
    assert!(!published.exists(), "the corrupted recreation is removed");
    assert!(room.join(&room_name).exists(), "the room's copy stays");
    assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 1);
}

/// **Each removal of a copy that does not read back says what it left**
/// (#157, Codex on #158 at 99d1184): a name the read-back found already
/// gone (`ENOENT`) is removed and synced; an unlink that took before a sync
/// that failed is removed and unsynced; an unlink that failed (the name a
/// directory) leaves the copy standing for the operator. The first two let
/// the next verb republish the room's copy; the third refuses until the
/// operator removes it. Perturbation: read `ENOENT` as a failed unlink and
/// the first reads `Standing`; fold the sync's failure into the unlink's
/// and the second reads `Standing`.
#[test]
fn each_removal_of_a_copy_that_does_not_read_back_says_what_it_left() {
    type Damage = fn(&std::path::Path);
    // The tag, the damage done before the read-back, whether the removal's
    // sync succeeds, the removal expected and what its line says.
    type Case = (
        &'static str,
        Damage,
        bool,
        fn(&Removal) -> bool,
        &'static str,
    );
    let gone: Damage = |path| std::fs::remove_file(path).unwrap();
    let flipped: Damage = |path| {
        let mut bytes = std::fs::read(path).unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        std::fs::write(path, &bytes).unwrap();
    };
    let a_directory: Damage = |path| {
        std::fs::remove_file(path).unwrap();
        std::fs::create_dir(path).unwrap();
    };
    let cases: [Case; 3] = [
        (
            "gone",
            gone,
            true,
            |r| *r == Removal::Synced,
            "the directory synced",
        ),
        (
            "unsynced",
            flipped,
            false,
            |r| matches!(r, Removal::Unsynced(_)),
            "did not sync",
        ),
        (
            "standing",
            a_directory,
            true,
            |r| matches!(r, Removal::Standing(_)),
            "was not removed",
        ),
    ];
    for (tag, damage, syncs, expected, says) in cases {
        let (_scratch, room, dir, mine, names) =
            room_of_save_points(&format!("read-back-removal-{tag}"), 1);
        let room_name = names[0].0.clone();
        let dir_fd = open_directory(&dir).unwrap();
        let mut published = None;
        let mut hooks = PublishHooks {
            after_scan: &mut || {},
            available: &mut available_bytes,
            cap: PUBLISH_CAP,
            room_bound: ROOM_SCAN_BOUND,
            finished: None,
            before_read_back: &mut |_, name| {
                damage(&dir.join(name));
                published = Some(dir.join(name));
            },
            removal_sync: &mut |directory| {
                if syncs {
                    sync_directory(directory)
                } else {
                    Err(std::io::Error::other("the injected sync failure"))
                }
            },
            removal: None,
        };
        let (lines, deferred) = publish_with(
            open_directory(&room).unwrap().as_fd(),
            mine.uid,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
            &mut hooks,
        )
        .unwrap();
        let removal = hooks.removal.clone().expect("a removal");
        let published = published.expect("the read-back hook ran");
        assert!(lines.is_empty() && deferred, "{tag}: no line, deferred");
        assert!(expected(&removal), "{tag}: {removal:?}");
        assert!(
            removal.says(&room_name).contains(says),
            "{tag}: the log line says what stands: {}",
            removal.says(&room_name)
        );
        assert!(
            room.join(&room_name).exists(),
            "{tag}: the room's copy stays"
        );

        let republish = || {
            publish_noting_deferral(
                open_directory(&room).unwrap().as_fd(),
                mine.uid,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
            )
            .unwrap()
        };
        if matches!(removal, Removal::Standing(_)) {
            assert!(published.is_dir(), "the copy stands under its name");
            let (lines, deferred) = republish();
            assert!(
                lines.is_empty() && deferred,
                "the next verb refuses on it until the operator removes it"
            );
            std::fs::remove_dir(&published).unwrap();
        } else {
            assert!(!published.exists(), "{tag}: the name is gone");
        }
        let (lines, deferred) = republish();
        assert!(!deferred, "{tag}: the next verb republishes");
        assert_eq!(lines.len(), 1, "{tag}: under its line");
    }
}
