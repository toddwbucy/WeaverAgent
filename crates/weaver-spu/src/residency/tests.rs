use super::*;
use crate::family::FamilyName;
use weaver_types::ArtifactRef;

/// A vocabulary that promotes what it was told to promote and splits the
/// rest, which is what a real tokenizer does to a marker it does not carry.
fn vocabulary(
    promoting: &'static [(&'static str, u32)],
) -> impl Fn(&str) -> Result<Vec<TokenId>, DecodeFault> {
    move |text: &str| {
        Ok(match promoting.iter().find(|(marker, _)| *marker == text) {
            Some((_, id)) => vec![TokenId(*id)],
            None => text.chars().map(|c| TokenId(c as u32)).collect(),
        })
    }
}

/// **The turn's close is the family's declaration, not the artifact's
/// end-of-sequence.**
///
/// The two coincide on every Qwen artifact, which declares `<|im_end|>` as
/// its EOS, and that coincidence is what let the composition root read the
/// artifact alone for as long as it did. The fixture is drawn from gemma4,
/// where they differ: `<eos>` is 1 and the turn closes with `<turn|>` at
/// 106. What the terminator carries decides what is made resident after a
/// generation, so a residency holding `<eos>` holds a turn the next
/// rendering does not continue from.
///
/// Perturbation: return the artifact's `eos` as the terminator and this
/// fails on the first assertion. Watched under exactly that.
#[test]
fn the_terminator_is_the_familys_close_rather_than_the_artifacts_eos() {
    let promoted = promote_stop_conditions(
        &["<turn|>"],
        TokenId(1),
        vocabulary(&[("<turn|>", 106), ("<eos>", 1)]),
    )
    .expect("a family whose close promotes has a stop set");

    assert_eq!(
        promoted.terminator,
        TokenId(106),
        "the turn's close is what the family declared"
    );
    assert!(
        promoted.tokens.contains(&TokenId(106)),
        "and it stops the generation"
    );
    assert!(
        promoted.tokens.contains(&TokenId(1)),
        "the artifact's end-of-sequence joins the set as a backstop rather \
         than replacing it"
    );
    assert!(promoted.unpromoted.is_empty());
}

/// **A declared condition the vocabulary splits is named rather than
/// carried.**
///
/// Llama is the standing case: `<|eom_id|>` arrived with 3.1 and the
/// tokenizer this workshop can reach is 3.0-era, which splits it into
/// several tokens. A split marker can never equal one drawn token, so
/// carrying it would be a stop condition that silently never fires.
///
/// Perturbation: push the first token of a split condition instead of
/// naming it, and the `unpromoted` assertion fails while a reader is told
/// the family's whole set is in force.
#[test]
fn a_condition_the_vocabulary_splits_is_named_rather_than_carried() {
    let promoted = promote_stop_conditions(
        &["<|eot_id|>", "<|eom_id|>"],
        TokenId(128009),
        vocabulary(&[("<|eot_id|>", 128009)]),
    )
    .expect("the close promotes even where a later condition does not");

    assert_eq!(promoted.terminator, TokenId(128009));
    assert_eq!(
        promoted.unpromoted,
        vec!["<|eom_id|>"],
        "the condition that could never fire is reported"
    );
    assert_eq!(
        promoted.tokens,
        vec![TokenId(128009)],
        "and it is not in the set, nor is the EOS twice"
    );
}

/// **A family whose turn close does not promote refuses rather than
/// running.**
///
/// Every turn would generate to the token cap and leave the close in the
/// emission as prose, which is a load that reports success and produces a
/// trace no reader can bracket.
#[test]
fn a_close_that_does_not_promote_refuses_the_open() {
    let outcome = promote_stop_conditions(&["<turn|>"], TokenId(1), vocabulary(&[]));
    assert!(
        matches!(outcome, Err(DecodeFault::Engine { .. })),
        "the open refuses, got {outcome:?}"
    );
}

/// **A second admit refuses on the ordering rather than re-running the
/// steps.** Nothing is idempotent: this crate begins empty, admits once,
/// and dies.
///
/// Perturbation: remove the `admit_attempted` guard from `admit` and this
/// test fails, because the second call runs step one again and refuses as
/// `Unresolvable` rather than as `AlreadyAttempted`. Watched under exactly
/// that removal.
#[test]
fn a_second_admit_refuses_on_the_ordering() {
    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef("/nonexistent/artifact".into()),
        devices: vec![DeviceOrdinal(0)],
    };
    let first = residency
        .admit(&binding, Headroom(0), 4096, ReadoutElection(false), false)
        .err();
    assert!(
        matches!(first, Some(AdmitRefusal::Unresolvable { .. })),
        "the fixture refuses at step one, got {first:?}"
    );
    let second = residency
        .admit(&binding, Headroom(0), 4096, ReadoutElection(false), false)
        .err();
    assert_eq!(
        second,
        Some(AdmitRefusal::AlreadyAttempted),
        "the second admit refuses on the ordering, not on the artifact"
    );
}

/// **Release with nothing resident refuses.**
#[test]
fn release_without_residency_refuses() {
    let mut residency = Residency::new();
    assert_eq!(residency.release(), Err(LifecycleRefusal::NoResidency));
}

/// A minimal GGUF whose header carries exactly the two keys the free steps
/// need, the architecture the family lookup reads and the block count the
/// layer division reads (device pair amendment PAIR-3 and PAIR-4), and
/// resolves to exactly one registry entry, so an admit against it walks past
/// steps one and two.
///
/// **It declares an uncontested architecture on purpose.** The fixture
/// carries a header and no chat template, and a contested architecture
/// needs the template to reach an entry, so declaring one here would make
/// this fixture refuse at selection and every test using it would read
/// that refusal instead of the condition it means to exercise. `qwen2` is
/// carried by one entry and needs no template to resolve.
///
/// **It answers a guard rather than a path**, which removes the scratch
/// directory when the test ends, pass or fail, as issue #704's guards do
/// across the other crates. Its `Drop` runs on the unwind a failed
/// assertion takes, so the caller binds it for the length of the test.
fn resolvable_gguf() -> Fixture {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("weaver-spu-residency-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let path = dir.join("resolvable.gguf");
    let mut file = Vec::new();
    file.extend_from_slice(b"GGUF");
    file.extend_from_slice(&3u32.to_le_bytes());
    file.extend_from_slice(&0u64.to_le_bytes());
    file.extend_from_slice(&4u64.to_le_bytes());
    let key = b"general.architecture";
    file.extend_from_slice(&(key.len() as u64).to_le_bytes());
    file.extend_from_slice(key);
    file.extend_from_slice(&8u32.to_le_bytes());
    let value = b"qwen2";
    file.extend_from_slice(&(value.len() as u64).to_le_bytes());
    file.extend_from_slice(value);
    // The block count and the attention shape PAIR-4's room reads: a header
    // with no head count leaves the layer kind unidentified and refuses.
    for (key, value) in [
        (&b"qwen2.block_count"[..], 2u32),
        (b"qwen2.embedding_length", 8),
        (b"qwen2.attention.head_count", 2),
    ] {
        file.extend_from_slice(&(key.len() as u64).to_le_bytes());
        file.extend_from_slice(key);
        file.extend_from_slice(&4u32.to_le_bytes());
        file.extend_from_slice(&value.to_le_bytes());
    }
    let mut handle = std::fs::File::create(&path).expect("a fixture file");
    handle.write_all(&file).expect("the fixture is written");
    Fixture { dir, path }
}

/// The fixture's path and the directory holding it, removed on drop.
struct Fixture {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// **An ordinal that appears twice refuses on the set's shape, on the admit
/// path.** Two shards of one artifact on one card is not a two-device
/// admission: judged as distinct, the pair would pass room on half the real
/// demand and skip the peer check.
///
/// The fixture resolves and its header reads, so the admit walks past the
/// free steps and the refusal read here is the shape judgment's own. A
/// first version of this test called `judge_distinct` directly and could
/// not fail under the call-site removal, which is why this one drives
/// `admit`.
///
/// Perturbation: remove the `judge_distinct` call from `admit` and this
/// test fails, because the refusal becomes the driver condition's rather
/// than the shape's. Watched under exactly that removal.
#[test]
fn a_duplicate_ordinal_refuses_on_the_admit_path() {
    let mut residency = Residency::new();
    let fixture = resolvable_gguf();
    let binding = ModelBinding {
        artifact: ArtifactRef(fixture.path.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0), DeviceOrdinal(0)],
    };
    assert_eq!(
        residency
            .admit(&binding, Headroom(0), 4096, ReadoutElection(false), false)
            .err(),
        Some(AdmitRefusal::DuplicateDevice { ordinal: 0 }),
        "the refusal is the set's shape, before any driver condition"
    );
}

/// The floor's refusal set carries no family-naming case, so the name is
/// lost at the boundary by design and kept on this side of it. This pins
/// that the conversion is total rather than lossy in some other way.
#[test]
fn a_family_refusal_crosses_the_seam_as_a_floor_case() {
    let refusal = AdmitRefusal::Family(FamilyRefusal::UnknownFamily(FamilyName("x".into())));
    assert_eq!(
        LifecycleRefusal::from(refusal),
        LifecycleRefusal::ArtifactUnreadable
    );
}

/// A fixture file in its own scratch directory, removed on drop.
fn written(tag: &str, bytes: &[u8]) -> Fixture {
    let dir =
        std::env::temp_dir().join(format!("weaver-spu-residency-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir");
    let path = dir.join(format!("{tag}.gguf"));
    std::fs::write(&path, bytes).expect("a fixture file");
    Fixture { dir, path }
}

/// A dense qwen2 artifact of two layers whose first layer is four times its
/// second, with one KV head of width 32 per layer.
fn dense_pair_artifact(tag: &str) -> Fixture {
    use crate::room::tests::{Meta, gguf};
    let meta = [
        ("general.architecture", Meta::Text("qwen2")),
        ("qwen2.block_count", Meta::U32(2)),
        ("qwen2.embedding_length", Meta::U32(64)),
        ("qwen2.attention.head_count", Meta::U32(2)),
        ("qwen2.attention.head_count_kv", Meta::U32(1)),
    ];
    let tensors: &[(&str, &[u64], u32)] = &[
        ("blk.0.ffn_up.weight", &[64, 256], 1),
        ("blk.1.ffn_up.weight", &[64, 64], 1),
        ("output.weight", &[64, 32], 1),
    ];
    written(tag, &gguf(&meta, tensors))
}

/// Admit `binding` against a stated environment and a recording driver,
/// answering the refusal.
fn refusal_on(
    binding: &ModelBinding,
    headroom: u64,
    visibility: Visibility,
    query: &crate::devices::tests::Recording,
) -> Option<AdmitRefusal> {
    let side = DeviceSide {
        visibility,
        query: Some(query),
    };
    Residency::new()
        .admit_on(
            binding,
            Headroom(headroom),
            4096,
            ReadoutElection(false),
            false,
            None,
            &side,
        )
        .err()
}

fn bound(fixture: &Fixture, devices: &[u32]) -> ModelBinding {
    ModelBinding {
        artifact: ArtifactRef(fixture.path.to_string_lossy().into_owned()),
        devices: devices.iter().copied().map(DeviceOrdinal).collect(),
    }
}

/// **A wrong mask refuses at admission, before any device query**, per the
/// device pair amendment PAIR-1: a missing order variable, and a mask that
/// is not the binding's, for a single device and for a pair. The recording
/// driver was asked nothing. The pair is what watches the identity
/// judgment's place, since a single device asks no identity.
///
/// Perturbation: drop the `judge_mask` call, or move it after the identity
/// or room judgment, and this test fails.
#[test]
fn a_wrong_mask_refuses_before_any_device_query() {
    use crate::devices::tests::{Recording, masked};
    let fixture = dense_pair_artifact("mask");
    let unordered = |mask: &str| Visibility {
        order: None,
        visible: Some(mask.into()),
    };
    let cases = [
        (&[0][..], "0", vec![unordered("0"), masked("1"), masked("")]),
        (
            &[0, 1][..],
            "0,1",
            vec![unordered("0,1"), masked("1,0"), masked("0")],
        ),
    ];
    for (devices, expected, wrong) in cases {
        let binding = bound(&fixture, devices);
        for found in wrong {
            let query = Recording::uniform(devices.len(), u64::MAX);
            let refusal = refusal_on(&binding, 0, found.clone(), &query);
            assert_eq!(
                refusal,
                Some(AdmitRefusal::Mask(crate::devices::MaskRefusal {
                    found,
                    expected: expected.into(),
                }))
            );
            assert!(query.asked.borrow().is_empty(), "{:?}", query.asked);
            assert_eq!(
                LifecycleRefusal::from(refusal.unwrap()),
                LifecycleRefusal::DeviceCannotAdmit
            );
        }
    }
}

/// **Mixed members refuse after the mask and before room**, per PAIR-2: a
/// pair whose second member differs in name refuses naming both members,
/// and the driver was asked for identities and nothing else.
///
/// Perturbation: skip the identity judgment for pairs, or judge room
/// first, and this test fails.
#[test]
fn mixed_members_refuse_before_room() {
    use crate::devices::tests::{Recording, card, masked};
    let fixture = dense_pair_artifact("mixed");
    let binding = bound(&fixture, &[0, 2]);
    let mut query = Recording::uniform(2, u64::MAX);
    query.identities[1] = Some(card("RTX 2000 Ada"));
    assert_eq!(
        refusal_on(&binding, 0, masked("0,2"), &query),
        Some(AdmitRefusal::DeviceRefused(DeviceRefusal::Mismatched {
            members: vec![(0, card("RTX A6000")), (2, card("RTX 2000 Ada"))],
        }))
    );
    assert_eq!(*query.asked.borrow(), ["identity 0", "identity 1"]);
}

/// **A refusal names the binding's ordinal, never the position**, on the
/// admit path: a binding of ordinal 7 whose device the driver cannot open
/// refuses as unreachable naming 7, though the driver was asked for
/// position 0.
///
/// Perturbation: report the position and this test fails.
#[test]
fn an_admit_refusal_names_the_binding_ordinal() {
    use crate::devices::tests::{Recording, masked};
    let fixture = dense_pair_artifact("ordinal");
    let query = Recording::uniform(0, 0);
    assert_eq!(
        refusal_on(&bound(&fixture, &[7]), 0, masked("7"), &query),
        Some(AdmitRefusal::DeviceRefused(DeviceRefusal::Unreachable {
            ordinal: 7
        }))
    );
    assert_eq!(*query.asked.borrow(), ["memory 0"]);
}

/// **Each device is judged against its own share on the admit path**, per
/// PAIR-4. The fixture's first layer is four times its second and the
/// output sits on the second device, with each layer's cache at 4096
/// positions. The second device is given one byte less than its share plus
/// the headroom and refuses, naming its ordinal and that need.
///
/// Perturbation: judge each device against the file over the width (the
/// even fraction) and this test fails: the even need is far below the
/// share, so the second device passes.
#[test]
fn each_device_is_judged_against_its_own_share() {
    use crate::devices::tests::{Recording, masked};
    let fixture = dense_pair_artifact("share");
    let binding = bound(&fixture, &[3, 5]);
    // One KV head of width 32, key and value, 4096 positions, F16.
    let cache = (32 + 32) * 4096 * 2;
    let first = 64 * 256 * 2 + cache;
    let second = 64 * 64 * 2 + 64 * 32 * 2 + cache;
    let headroom = 100;
    let mut query = Recording::uniform(2, 0);
    query.memory[0] = Some((first + headroom, 48 << 30));
    query.memory[1] = Some((second + headroom - 1, 48 << 30));
    assert_eq!(
        refusal_on(&binding, headroom, masked("3,5"), &query),
        Some(AdmitRefusal::DeviceRefused(DeviceRefusal::NoRoom {
            ordinal: 5,
            free: second + headroom - 1,
            needed: second + headroom,
            total: 48 << 30,
        }))
    );
}

/// **The native artifact and the recurrent or hybrid GGUF keep the even
/// fraction**, each device charged the file over the width plus the whole
/// adapter, as before this act. The GGUF still divides by its layer count,
/// because the PAIR-3 split binds every multi-device GGUF load; only its
/// room charge is deferred to the second child act.
///
/// Perturbation: charge the hybrid by its attention shares, or give the
/// native artifact a division, and this test fails.
#[test]
fn native_and_hybrid_artifacts_keep_the_even_fraction() {
    use crate::room::tests::{Meta, gguf};
    let devices = [DeviceOrdinal(0), DeviceOrdinal(1)];
    let native = plan(None, &devices, 4096, None, || Ok(1000)).expect("a plan");
    assert_eq!(native.division, None);
    assert_eq!(native.shares, None);
    assert_eq!(native.needs, [500, 500]);

    let meta = [
        ("general.architecture", Meta::Text("qwen35")),
        ("qwen35.block_count", Meta::U32(4)),
        ("qwen35.embedding_length", Meta::U32(64)),
        ("qwen35.attention.head_count", Meta::U32(2)),
    ];
    let fixture = written("hybrid", &gguf(&meta, &[]));
    let layout = Layout::read(&[&fixture.path]).expect("the layout reads");
    let hybrid = plan(Some(&layout), &devices, 4096, None, || Ok(1000)).expect("a plan");
    assert_eq!(hybrid.division, Division::even(4, 2));
    assert_eq!(hybrid.shares, None);
    assert_eq!(hybrid.needs, [500, 500]);
}

/// **A tensor directory that does not parse keeps its account**: the
/// refusal carries no detail, so the typed standard-error line names the
/// shard and the reader's own words.
///
/// Perturbation: drop the detail from the line and this test fails.
#[test]
fn a_layout_fault_line_names_the_shard_and_the_detail() {
    let line = layout_fault_line(
        Path::new("/models/m-00002-of-00003.gguf"),
        "tensor blk.7.attn_q.weight has type 99",
    );
    let value: serde_json::Value = serde_json::from_str(&line).expect("one typed line");
    assert_eq!(
        value,
        serde_json::json!({"layout_fault": {
            "shard": "/models/m-00002-of-00003.gguf",
            "detail": "tensor blk.7.attn_q.weight has type 99",
        }})
    );
    assert!(!line.contains('\n'));
}
