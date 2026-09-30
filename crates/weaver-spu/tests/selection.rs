//! conforms: spu-family-is-architecture-and-template
//!
//! The two-field key of `weaver-spu-Spec` section 5, against held artifacts.
//!
//! **These read real artifacts**, because the claim is about what a shipped
//! model declares and a synthetic header asserting it would be this crate
//! agreeing with itself.
#![cfg(feature = "gguf")]

use std::path::{Path, PathBuf};
use weaver_spu::artifact;
use weaver_spu::family::{self, FamilyName};

/// An artifact these tests read, and where to find it.
struct Fixture {
    /// The variable an operator names it with. **One variable per fixture**,
    /// after `markers.rs`. `WEAVER_TEST_GGUF` is not reused: it names the model
    /// `loaded.rs` loads, and a run overriding that to exercise the seam would
    /// otherwise redirect these tests to an artifact whose family is not the
    /// one they assert about.
    env: &'static str,
    /// Where this workshop keeps it when nobody says otherwise.
    default_path: &'static str,
}

impl Fixture {
    /// The artifact, or `None` where this box does not carry it.
    ///
    /// **A missing default and a missing override are not the same absence**,
    /// which is the distinction `markers.rs` draws and the reason it draws it.
    /// A default that is not there is a workshop without that artifact, and
    /// the test has nothing to say. An override that is not there is an
    /// operator who asked for a measurement and would get a pass instead,
    /// which must not happen quietly.
    fn resolve(&self) -> Option<PathBuf> {
        match std::env::var_os(self.env) {
            Some(named) => {
                let path = PathBuf::from(named);
                assert!(
                    path.is_file(),
                    "{} names {}, which is not a regular file",
                    self.env,
                    path.display()
                );
                Some(path)
            }
            None => {
                let path = Path::new(self.default_path).to_path_buf();
                path.is_file().then_some(path)
            }
        }
    }
}

/// The artifact that declares `llama` and renders ChatML, which is the pairing
/// this act exists for.
const CHATML_LLAMA: Fixture = Fixture {
    env: "WEAVER_ARTIFACT_SMOLLM2",
    default_path: "/opt/weaver/models/smollm2-360m-instruct-q8_0.gguf",
};

/// The third reading of the llama architecture: Mistral Small declares
/// `llama` and renders `[INST]`.
const INST_LLAMA: Fixture = Fixture {
    env: "WEAVER_ARTIFACT_MISTRAL_SMALL",
    default_path: "/opt/weaver/models/h-dist/Mistral-Small-3.2-24B-Instruct-2506-Q4_K_M.gguf",
};

/// The artifact whose template the detector refuses, which is what keeps the
/// render off the uncontested path.
const DETECTOR_REFUSES: Fixture = Fixture {
    env: "WEAVER_ARTIFACT_GEMMA4",
    default_path: "/opt/weaver/models/gemma4-31b-it-Q8_0.gguf",
};

/// The tag half of the contested phi3 pair: Phi-4-mini, whose template
/// builds its markers by concatenation, so this fixture is also the standing
/// proof that the detector answers where a template-text scan would not.
const PHI3_TAG: Fixture = Fixture {
    env: "WEAVER_ARTIFACT_PHI4_MINI",
    default_path: "/opt/weaver/models/microsoft_Phi-4-mini-instruct-Q4_K_M.gguf",
};

/// The separator half: Phi-4 14B, same architecture string, disjoint markers.
const PHI3_SEP: Fixture = Fixture {
    env: "WEAVER_ARTIFACT_PHI4",
    default_path: "/opt/weaver/models/phi-4-Q4_K_S.gguf",
};

fn header_of(path: &Path) -> artifact::ArtifactHeader {
    let mut pinned = artifact::pin(path).expect("the artifact pins");
    artifact::read_header(&mut pinned).expect("the header reads")
}

/// **The case that opened #88 and closed as #175: an artifact declaring
/// `llama` and rendering ChatML.**
///
/// Before this act the architecture reached one entry, which handed SmolLM2
/// the Llama 3 stop set it was never trained against and refused the load. The
/// architecture now reaches two, and what separates them is what the artifact
/// renders.
#[test]
fn a_chatml_artifact_declaring_llama_selects_the_chatml_entry() {
    let Some(path) = CHATML_LLAMA.resolve() else {
        return;
    };
    let header = header_of(&path);
    assert_eq!(header.family, FamilyName("llama".into()));
    let template = header
        .chat_template
        .as_deref()
        .expect("the artifact declares a chat template");

    let selected = family::select(&header.family, Some(template)).expect("the artifact selects");
    assert!(
        selected.selecting_markers.contains(&"<|im_start|>"),
        "the ChatML entry is the one selected, not the Llama 3 entry"
    );
    assert!(
        !selected.selecting_markers.contains(&"<|begin_of_text|>"),
        "the Llama 3 stop set is what refused this artifact before the act"
    );
}

/// **The refusal the 2026-08-17 sweep recorded, resolved by the third row.**
///
/// Mistral Small declares `llama` and renders `[INST]`, matching neither the
/// Llama 3 set nor the ChatML set, so cell 11 of the sweep refused it in
/// every model-needing test (#186). The selecting set is the module's
/// `SELECTING_MARKERS`, without `<s>`, because a rendering never emits the
/// BOS - the set with `<s>` in it would leave this artifact refused with the
/// row present.
#[test]
fn an_inst_artifact_declaring_llama_selects_the_inst_entry() {
    let Some(path) = INST_LLAMA.resolve() else {
        return;
    };
    let header = header_of(&path);
    assert_eq!(header.family, FamilyName("llama".into()));
    let template = header
        .chat_template
        .as_deref()
        .expect("the artifact declares a chat template");

    let selected = family::select(&header.family, Some(template)).expect("the artifact selects");
    assert!(
        selected.selecting_markers.contains(&"[INST]"),
        "the [INST] entry is the one selected"
    );
    assert!(
        !selected.selecting_markers.contains(&"<|im_start|>")
            && !selected.selecting_markers.contains(&"<|begin_of_text|>"),
        "neither sibling's set is the one selected"
    );
}

/// **A contested architecture with no template refuses rather than picking.**
///
/// The fact that would choose is absent, and answering by position is what the
/// compile-time pin and this refusal both exist to prevent.
#[test]
fn a_contested_architecture_without_a_template_refuses() {
    let outcome = family::select(&FamilyName("llama".into()), None);
    assert!(
        matches!(outcome, Err(family::FamilyRefusal::TemplateAbsent(_))),
        "expected a refusal naming the absent template, got {outcome:?}"
    );
}

/// **An architecture the template detector cannot read stays admissible.**
///
/// Gemma4's template returns an error rather than a rendering, measured against
/// the artifact this workshop holds. It resolves because its architecture is
/// carried by one entry and nothing is rendered to reach it, which is the
/// clause of Spec section 5 that keeps an unconditional render out.
#[test]
fn a_family_the_detector_refuses_still_resolves() {
    let Some(path) = DETECTOR_REFUSES.resolve() else {
        return;
    };
    let header = header_of(&path);
    assert_eq!(header.family, FamilyName("gemma4".into()));
    family::select(&header.family, header.chat_template.as_deref())
        .expect("an uncontested architecture resolves without rendering");
}

/// **An architecture no entry declares refuses naming it**, which the two-field
/// key leaves unchanged.
#[test]
fn an_uncarried_architecture_still_refuses_by_name() {
    let absent = FamilyName("not-a-family".into());
    assert!(matches!(
        family::select(&absent, None),
        Err(family::FamilyRefusal::UnknownFamily(name)) if name == absent
    ));
}

/// **The second contested architecture, selected from a vendor's own line.**
///
/// Phi-4-mini declares `phi3` and renders role tags whose literals its
/// template source does not carry - `<|user|>` is built from `'<|' + role +
/// '|>'` - so this is the case where reading the template text finds two of
/// three markers and the detector's rendering finds all of them. The entry
/// selected must be the tag row and must not be the separator row.
#[test]
fn a_tag_artifact_declaring_phi3_selects_the_tag_entry() {
    let Some(path) = PHI3_TAG.resolve() else {
        return;
    };
    let header = header_of(&path);
    assert_eq!(header.family, FamilyName("phi3".into()));
    let template = header
        .chat_template
        .as_deref()
        .expect("the artifact declares a chat template");

    let selected = family::select(&header.family, Some(template)).expect("the artifact selects");
    assert!(
        selected.selecting_markers.contains(&"<|user|>"),
        "the tag entry is the one selected"
    );
    assert!(
        !selected.selecting_markers.contains(&"<|im_sep|>"),
        "and not the separator entry"
    );
}

/// The separator half of the pair, from the artifact that declares the same
/// architecture and renders the other format.
#[test]
fn a_separator_artifact_declaring_phi3_selects_the_separator_entry() {
    let Some(path) = PHI3_SEP.resolve() else {
        return;
    };
    let header = header_of(&path);
    assert_eq!(header.family, FamilyName("phi3".into()));
    let template = header
        .chat_template
        .as_deref()
        .expect("the artifact declares a chat template");

    let selected = family::select(&header.family, Some(template)).expect("the artifact selects");
    assert!(
        selected.selecting_markers.contains(&"<|im_sep|>"),
        "the separator entry is the one selected"
    );
    assert!(
        !selected.selecting_markers.contains(&"<|user|>"),
        "and not the tag entry"
    );
}
