use super::*;

fn taken(run: &str, sequence: u64, image: &[u8]) -> SavePoint {
    SavePoint::take(
        Stamp {
            run: run.into(),
            sequence,
            turn: 2,
        },
        "CREATE TABLE event (id INTEGER PRIMARY KEY)",
        image.to_vec(),
    )
}

/// **A save point round-trips through its bytes and its digest is its
/// name**, and two taken of one position differ by digest. Perturbation:
/// drop `taken` from the header and the two names collide.
#[test]
fn a_save_point_round_trips_and_two_of_one_position_differ() {
    let first = taken("r-1", 41, b"image bytes");
    let parsed = SavePoint::parse(&first.bytes()).expect("parses");
    assert_eq!(parsed, first);
    assert_eq!(parsed.digest(), first.digest());
    assert_eq!(first.name(), format!("{}.save-point", first.digest()));
    assert_eq!(
        first.schema,
        schema_digest("CREATE TABLE event (id INTEGER PRIMARY KEY)")
    );
    let second = taken("r-1", 41, b"image bytes");
    assert_ne!(
        first.name(),
        second.name(),
        "two save points at one position never share a name"
    );
    assert!(
        first.header.contains("\"ordinal\":") && second.header != first.header,
        "the nonce carries the process's own count: {}",
        first.header
    );
}

/// **A torn or damaged save point reads as corrupt and never as
/// holdings**: truncated part way, a byte flipped in the image, a byte
/// flipped in the stamp, and a check line altered each refuse.
/// Perturbation: skip the check comparison in `parse` and the flipped
/// cases parse as sound.
/// **The corpus holds the two readers equal**, per `weaver-admin-Spec`
/// section 4: every file of the workspace's save-point corpus parses to
/// the verdict the corpus gives it, so a case this parse admits and
/// admin's judgment refuses, or the reverse, fails here or in admin's
/// own corpus test. Perturbation: relax any one rule of `parse` and its
/// case parses sound against a `refuses` verdict.
#[test]
fn the_corpus_holds_the_parse_equal_to_admins_judgment() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../weaver-types/tests/fixtures/save-points");
    let verdicts = std::fs::read_to_string(corpus.join("verdicts.txt")).expect("the verdicts");
    let mut cases = 0;
    for line in verdicts.lines().filter(|l| !l.trim().is_empty()) {
        let (name, verdict) = line.split_once(' ').expect("name and verdict");
        let bytes = std::fs::read(corpus.join(name)).expect(name);
        let parsed = SavePoint::parse(&bytes);
        match verdict {
            "sound" => assert!(parsed.is_ok(), "{name} parses sound: {parsed:?}"),
            "refuses" => assert!(parsed.is_err(), "{name} refuses"),
            other => panic!("{name}: verdict {other} is not sound or refuses"),
        }
        cases += 1;
    }
    assert!(cases >= 10, "the corpus holds its cases: {cases}");
}

#[test]
fn a_torn_or_damaged_save_point_is_refused() {
    let sound = taken("r-1", 41, b"a longer image so a cut lands inside it");
    let bytes = sound.bytes();
    let truncated = &bytes[..bytes.len() - 5];
    assert!(matches!(
        SavePoint::parse(truncated),
        Err(SavePointFault::Malformed(_))
    ));
    let mut flipped = bytes.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    assert_eq!(SavePoint::parse(&flipped), Err(SavePointFault::CheckFailed));
    let mut stamp_flipped = bytes.clone();
    let at = sound
        .header
        .find("\"sequence\":41")
        .expect("the stamp spells its sequence")
        + "\"sequence\":".len();
    stamp_flipped[at] = b'7';
    assert_eq!(
        SavePoint::parse(&stamp_flipped),
        Err(SavePointFault::CheckFailed),
        "an altered stamp is corrupt, never a position"
    );
    // A check line spelled another way, the same check inside it, is
    // not this format: a parsed save point's digest is the file's.
    // Perturbation: parse the check line as JSON and compare the named
    // check, and the respelled file parses with a digest that is not
    // sha256 of its bytes.
    let line_end = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
    let check_end = line_end + bytes[line_end..].iter().position(|&b| b == b'\n').unwrap();
    let mut respelled = bytes[..line_end].to_vec();
    respelled.extend_from_slice(b"{ \"check\" : ");
    respelled.extend_from_slice(&bytes[line_end + 9..check_end - 1]);
    respelled.extend_from_slice(b" }");
    respelled.extend_from_slice(&bytes[check_end..]);
    assert_eq!(
        SavePoint::parse(&respelled),
        Err(SavePointFault::CheckFailed)
    );
    // **The stamp line is exactly the format's**: a stamp without its
    // nonce, with a malformed nonce, with an extra member, or missing
    // one, is not this format even with a check written to match.
    // Perturbation: read the members by name without the count and
    // the extra-member and absent-nonce stamps parse.
    let restamped = |edit: &dyn Fn(&mut serde_json::Map<String, serde_json::Value>)| {
        let mut stamp: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&sound.header).unwrap();
        edit(&mut stamp);
        let header = serde_json::Value::Object(stamp).to_string();
        let mut hasher = sha2::Sha256::new();
        hasher.update(header.as_bytes());
        hasher.update(b"\n");
        hasher.update(&sound.image);
        let check = serde_json::json!({"check": hex(&hasher.finalize())}).to_string();
        let mut bytes = header.into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(check.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(&sound.image);
        bytes
    };
    type Edit<'a> = &'a dyn Fn(&mut serde_json::Map<String, serde_json::Value>);
    let cases: [(&str, Edit); 5] = [
        ("no taken", &|m| {
            m.remove("taken");
        }),
        ("malformed taken", &|m| {
            m.insert("taken".into(), serde_json::json!({"pid": 1}));
        }),
        ("wall_ns not digits", &|m| {
            m.insert(
                "taken".into(),
                serde_json::json!({"pid": 1, "ordinal": 0, "wall_ns": "soon"}),
            );
        }),
        ("extra member", &|m| {
            m.insert("note".into(), serde_json::json!("x"));
        }),
        ("no schema", &|m| {
            m.remove("schema");
        }),
    ];
    for (label, edit) in cases {
        assert!(
            matches!(
                SavePoint::parse(&restamped(edit)),
                Err(SavePointFault::Malformed(_))
            ),
            "{label} is not this format"
        );
    }
    assert_eq!(
        SavePoint::parse(&restamped(&|_| {})).expect("the unedited stamp still parses"),
        sound
    );
    assert_eq!(
        SavePoint::parse(&bytes).expect("sound").digest(),
        hex(&sha2::Sha256::digest(&bytes)),
        "a parsed save point's digest is sha256 of the file"
    );
    assert!(matches!(
        SavePoint::parse(b"not a save point"),
        Err(SavePointFault::Malformed(_))
    ));
    assert!(matches!(
        SavePoint::parse(b"{}\n{}\n"),
        Err(SavePointFault::Malformed(_))
    ));
}

/// **The room writes a new file under the digest and never over one**:
/// the file parses back to the save point, and a second write of the
/// same bytes is refused rather than overwriting.
/// Perturbation: replace the link with a rename and the second write
/// succeeds over the first.
#[test]
fn the_room_writes_new_files_and_overwrites_none() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-state-room-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("room");
    let room = Room::open(&dir).expect("opens");
    let save_point = taken("r-1", 41, b"image");
    let name = room.write(&save_point).expect("writes");
    assert_eq!(name, save_point.name());
    assert!(dir.join(&name).is_file());
    assert!(
        !dir.join(format!(".part-{name}")).exists(),
        "the part name is gone"
    );
    assert_eq!(
        SavePoint::parse(&std::fs::read(dir.join(&name)).expect("reads")).expect("parses"),
        save_point
    );
    assert!(
        matches!(room.write(&save_point), Err(SavePointFault::Io(_))),
        "the same bytes again are refused, never overwritten"
    );
    // The failed-sync cleanup is checked and names the file: a removal
    // of a name the room does not hold reports which name. Perturbation:
    // ignore the unlink's result in `remove_finished` and the absent
    // name reports success.
    let why = room
        .remove_finished("absent.save-point")
        .expect_err("an absent finished name cannot be removed");
    assert!(why.contains("unlink absent.save-point"), "{why}");
    room.remove_finished(&name)
        .expect("the finished name is removed and the room synced");
    assert!(!dir.join(&name).exists());
    let surface = room.surface().expect("surface");
    assert_eq!(surface.len(), 2);
    assert!(surface[0].starts_with("owner "));
    assert!(surface[1].starts_with("mode 0"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The save point's bound is the ruling's number, named**, per
/// `weaver-state-Spec` section 3 on the operator's ruling of 2026-10-08:
/// admin's `SAVE_POINT_BOUND` is pinned to the same literal under its own
/// watch, so the two readers are held equal by the number and neither
/// links the other. Perturbation: change the constant and this fails.
#[test]
fn the_save_point_bound_is_one_gibibyte() {
    assert_eq!(SAVE_POINT_BOUND, 1_073_741_824);
}

/// **A save point past the bound writes no part**: with the bound
/// lowered under the file's size the write is refused naming the size,
/// the part a previous write left is cleared, and the room holds no
/// file; at its own size the same save point writes. The size is the
/// rendered file's, counted without rendering it. Perturbation: drop the
/// bound check from `write_part_within` and the lowered write stands a
/// part.
#[test]
fn a_save_point_past_the_bound_writes_no_part() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-state-bound-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("room");
    let room = Room::open(&dir).expect("opens");
    let save_point = taken("r-1", 41, b"an image of some bytes");
    let size = save_point.bytes().len() as u64;
    assert_eq!(save_point.rendered_len(), size, "the count is the file's");
    let earlier = room
        .write_part(&taken("r-1", 40, b"earlier"))
        .expect("an earlier part");
    assert!(dir.join(format!(".part-{earlier}")).exists());
    assert_eq!(
        room.write_part_within(&save_point, size - 1),
        Err(SavePointFault::PastBound {
            size,
            bound: size - 1
        })
    );
    let left: Vec<_> = std::fs::read_dir(&dir)
        .expect("lists")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert!(left.is_empty(), "the refusal leaves no file: {left:?}");
    let name = room
        .write_part_within(&save_point, size)
        .expect("at the bound it writes");
    assert!(dir.join(format!(".part-{name}")).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A stamp the readers would refuse writes no part** (#99, K7): a run
/// of 129 bytes, or a schema that is not 64 lowercase hex, is refused
/// `Malformed` by the parse's own rule before any part is created, the
/// part a previous write left cleared too, so the room holds no file; a
/// run of 128 writes. Perturbation: drop the `stamp_bounds` call from
/// `write_part_within` and the long run stands a part.
#[test]
fn a_stamp_the_readers_refuse_writes_no_part() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-state-stamp-bounds-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("room");
    let room = Room::open(&dir).expect("opens");
    room.write_part(&taken("r-1", 40, b"earlier"))
        .expect("an earlier part");
    let long = taken(&"r".repeat(129), 41, b"image");
    assert!(
        SavePoint::parse(&long.bytes()).is_err(),
        "the reader refuses it"
    );
    assert!(matches!(
        room.write_part(&long),
        Err(SavePointFault::Malformed(why)) if why.contains("128 printable")
    ));
    let mut foreign = taken("r-1", 41, b"image");
    foreign.schema = "NOT-HEX".into();
    assert!(matches!(
        room.write_part(&foreign),
        Err(SavePointFault::Malformed(why)) if why.contains("64 lowercase hex")
    ));
    let left: Vec<_> = std::fs::read_dir(&dir)
        .expect("lists")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert!(left.is_empty(), "the refusals leave no file: {left:?}");
    room.write_part(&taken(&"r".repeat(128), 41, b"image"))
        .expect("a run of 128 writes");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The descriptor is probed before it is adopted**: a closed number is
/// no save point, a regular file holding one reads, and a number holding
/// a directory is refused. Perturbation: drop the regular-file check and
/// the directory case reads as an io error from the read rather than a
/// refusal, which this test tells apart by the message.
#[test]
fn the_descriptor_is_probed_before_it_is_adopted() {
    use std::os::fd::IntoRawFd;
    let dir = std::env::temp_dir().join(format!(
        "weaver-state-spfd-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("one.save-point");
    let save_point = taken("r-1", 3, b"img");
    std::fs::write(&path, save_point.bytes()).expect("writes");
    let file = std::fs::File::open(&path).expect("opens");
    // A fresh high number, so no concurrent test's descriptor is touched.
    // SAFETY: F_DUPFD on a descriptor this test owns.
    let fd = unsafe { nix::libc::fcntl(file.into_raw_fd(), nix::libc::F_DUPFD, 800) };
    assert!(fd >= 800);
    let read = read_descriptor(fd).expect("reads").expect("holds one");
    assert_eq!(read, save_point);
    // Adopted and closed by the read, so the number now holds nothing.
    assert_eq!(read_descriptor(fd).expect("probes"), None);
    let directory = std::fs::File::open(&dir).expect("opens the dir");
    let fault = read_descriptor(directory.into_raw_fd()).expect_err("refused");
    assert_eq!(
        fault,
        SavePointFault::Io("not a regular file".into()),
        "a directory at the number is refused"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
