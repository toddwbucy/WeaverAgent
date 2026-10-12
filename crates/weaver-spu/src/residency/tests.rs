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
        .admit(&binding, Headroom(0), ReadoutElection(false), false)
        .err();
    assert!(
        matches!(first, Some(AdmitRefusal::Unresolvable { .. })),
        "the fixture refuses at step one, got {first:?}"
    );
    let second = residency
        .admit(&binding, Headroom(0), ReadoutElection(false), false)
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

/// A minimal GGUF whose header carries exactly the one key the family
/// lookup needs and resolves to exactly one registry entry, so an admit
/// against it walks past steps one and two.
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
    file.extend_from_slice(&1u64.to_le_bytes());
    let key = b"general.architecture";
    file.extend_from_slice(&(key.len() as u64).to_le_bytes());
    file.extend_from_slice(key);
    file.extend_from_slice(&8u32.to_le_bytes());
    let value = b"qwen2";
    file.extend_from_slice(&(value.len() as u64).to_le_bytes());
    file.extend_from_slice(value);
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
            .admit(&binding, Headroom(0), ReadoutElection(false), false)
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
