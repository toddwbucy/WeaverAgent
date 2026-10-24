//! Byte identity and compatibility admission for the separate STE100 duty.
//!
//! The declared-path entry acquires protected inputs through the artifact owner.
//! Supplied-byte admission exists only in test builds; production proof requires
//! acquisition of the declared path.
//! A compiled adapter must still load the model and compile both grammars before
//! admission can produce Ready. Fixture adapters exist only in test builds.

use std::collections::BTreeMap;
use std::sync::Arc;

use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{
    DecimalU64, Digest, STE100_PROTOCOL, SchemeDeclaration, Ste100AdmissionCode,
    Ste100AdmissionRefused, Ste100Bootstrap, Ste100Instruction, Ste100ModelManifest, Ste100Ready,
    Ste100Request, Ste100Result, Ste100Scheme, decode_strict,
};

#[cfg(test)]
use weaver_types::state_flow::ArtifactFile;

/// An immutable file supplied by the artifact owner, under its artifact-relative name.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct ArtifactBytes<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}

/// Exact manifest bytes and the files made available by their custody owner.
///
/// Supplying these bytes does not prove their filesystem origin. Production
/// bootstrap uses admit_from_path and its confined acquisition owner. This
/// supplied-byte representation is absent from production builds.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct SuppliedArtifacts<'a> {
    pub manifest: &'a [u8],
    pub files: &'a [ArtifactBytes<'a>],
}

/// Only manifest-referenced files whose complete bytes matched their declared digest.
#[derive(Debug)]
pub struct VerifiedFiles {
    pub(super) files: BTreeMap<String, Arc<Vec<u8>>>,
}

impl VerifiedFiles {
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(|bytes| bytes.as_slice())
    }

    /// Retain the same checked immutable bytes for a deferred adapter read.
    pub fn retain(&self, name: &str) -> Option<Arc<Vec<u8>>> {
        self.files.get(name).cloned()
    }
}

/// Keep the admitted inputs with the backend until its last read and drop.
/// No public operation extracts or mutably exposes the bare backend. Concrete
/// adapter election remains an O02-2 integration duty. The runtime invokes its
/// narrow whole-block operation without extracting this backend or its inputs.
///
/// ```compile_fail
/// use weaver_spu::ste100::admission::RetainedBackend;
/// fn escape<B: Default>(owner: &mut RetainedBackend<B>) -> B {
///     std::mem::take(owner.backend_mut())
/// }
/// ```
/// ```compile_fail
/// use weaver_spu::ste100::admission::RetainedBackend;
/// fn escape<B: Clone>(owner: &RetainedBackend<B>) -> B {
///     owner.backend().clone()
/// }
/// ```
#[derive(Debug)]
pub struct RetainedBackend<B> {
    // Drop the backend first: its destructor may still read admitted inputs.
    backend: B,
    files: VerifiedFiles,
    manifest: Arc<Vec<u8>>,
}

impl<B> RetainedBackend<B> {
    #[cfg(test)]
    pub(super) fn backend(&self) -> &B {
        &self.backend
    }
    pub fn files(&self) -> &VerifiedFiles {
        &self.files
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest
    }
}

/// Only compiled SPU adapters can supply this operation. The runtime constructs
/// and invokes the backend on the same worker thread; a backend need not be Send.
/// It returns one whole-block result, never incrementally publishable bindings.
pub(super) trait CheckBackend {
    fn check(&mut self, request: &Ste100Request) -> Ste100Result;
}

impl<B> RetainedBackend<B> {
    fn check(&mut self, request: &Ste100Request) -> Ste100Result
    where
        B: CheckBackend,
    {
        self.backend.check(request)
    }
}

/// The one checked bootstrap handoff from the transport to admission. Its fields
/// cannot be filled from a caller's parsed object or a claimed receive limit.
///
/// ```compile_fail
/// use weaver_spu::ste100::admission::ValidatedBootstrap;
/// use weaver_types::state_flow::Ste100Bootstrap;
/// fn forge(bootstrap: Ste100Bootstrap) -> ValidatedBootstrap {
///     ValidatedBootstrap { bootstrap }
/// }
/// ```
#[derive(Debug, PartialEq)]
pub struct ValidatedBootstrap {
    bootstrap: Ste100Bootstrap,
}

impl ValidatedBootstrap {
    pub fn bootstrap(&self) -> &Ste100Bootstrap {
        &self.bootstrap
    }
}

/// Immutable readiness identity minted only by actual admission. It may cross
/// from the worker while the non-Send backend and retained inputs stay there.
pub(super) struct AdmissionIdentity {
    bootstrap: Ste100Bootstrap,
    ready: Ste100Ready,
    verbs: Vec<String>,
}

impl AdmissionIdentity {
    pub(super) fn bootstrap(&self) -> &Ste100Bootstrap {
        &self.bootstrap
    }
    pub(super) fn ready(&self) -> &Ste100Ready {
        &self.ready
    }
    pub(super) fn verbs(&self) -> &[String] {
        &self.verbs
    }
}

/// Parsed, byte-checked admission material. This is not a language certificate.
#[derive(Debug)]
pub struct VerifiedArtifacts {
    bootstrap: Ste100Bootstrap,
    scheme: Ste100Scheme,
    declaration: SchemeDeclaration,
    manifest: Ste100ModelManifest,
    grammar: Vec<u8>,
    conditions: Vec<u8>,
    #[cfg(test)]
    grammar_lifetime: Arc<()>,
}

impl VerifiedArtifacts {
    pub fn bootstrap(&self) -> &Ste100Bootstrap {
        &self.bootstrap
    }

    pub fn scheme(&self) -> &Ste100Scheme {
        &self.scheme
    }

    pub fn declaration(&self) -> &SchemeDeclaration {
        &self.declaration
    }

    pub fn manifest(&self) -> &Ste100ModelManifest {
        &self.manifest
    }

    pub fn grammar(&self) -> &[u8] {
        &self.grammar
    }

    pub fn conditions(&self) -> &[u8] {
        &self.conditions
    }

    /// Test-only ownership witness; it exposes no grammar bytes or admission authority.
    #[cfg(test)]
    pub(super) fn grammar_lifetime(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.grammar_lifetime)
    }
}

mod sealed {
    pub trait Sealed {}
}

// Implementations belong to this duty's compiled adapter modules. A downstream
// caller cannot declare production compatibility by implementing the public trait.
pub(super) use sealed::Sealed as SealedAdapter;

/// Whether a compiled adapter belongs to production or the unit-test constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    Production,
    #[cfg(test)]
    TestOnly,
}

/// The compiled adapter's actual admission work, not a list of accepted strings.
///
/// The adapter must compile the supplied grammar and conditions, refuse unsupported
/// constructs, load only the verified model files, check its family/alignment ABI
/// and capacity, and instantiate the declared model instances. It returns a backend
/// only after those operations succeed. Artifact qualification remains external.
///
/// A downstream caller cannot register a permissive adapter:
///
/// ```compile_fail
/// use weaver_spu::ste100::admission::{
///     AdapterAdmission, AdapterKind, VerifiedArtifacts, VerifiedFiles,
/// };
/// use weaver_types::state_flow::Ste100AdmissionRefused;
/// struct Unchecked;
/// impl AdapterAdmission for Unchecked {
///     type Backend = ();
///     fn kind(&self) -> AdapterKind { AdapterKind::Production }
///     fn admit(&self, _: &VerifiedArtifacts, _: &VerifiedFiles)
///         -> Result<(), Ste100AdmissionRefused> { Ok(()) }
/// }
/// ```
pub trait AdapterAdmission: SealedAdapter {
    type Backend;

    fn kind(&self) -> AdapterKind;

    /// Refuse known unsupported operation before acquiring or parsing artifacts.
    /// The adapter owns this refusal. Success certifies neither compatibility nor
    /// admission, which still checks the exact retained files and grammars.
    fn precheck(&self) -> Result<(), Ste100AdmissionRefused> {
        Ok(())
    }

    fn admit(
        &self,
        artifacts: &VerifiedArtifacts,
        files: &VerifiedFiles,
    ) -> Result<Self::Backend, Ste100AdmissionRefused>;
}

/// Readiness is constructible only after byte checks and compiled-adapter admission.
///
/// ```compile_fail
/// use weaver_spu::ste100::admission::{Admitted, VerifiedArtifacts};
/// use weaver_types::state_flow::Ste100Ready;
/// fn forge(artifacts: VerifiedArtifacts, ready: Ste100Ready) -> Admitted<()> {
///     Admitted { artifacts, ready, backend: () }
/// }
/// ```
#[derive(Debug)]
pub struct Admitted<B> {
    // Backend destruction precedes every admitted input, including grammars.
    backend: RetainedBackend<B>,
    artifacts: VerifiedArtifacts,
    ready: Ste100Ready,
}

impl<B> Admitted<B> {
    pub fn ready(&self) -> &Ste100Ready {
        &self.ready
    }

    pub fn bootstrap(&self) -> &Ste100Bootstrap {
        self.artifacts.bootstrap()
    }

    pub fn verbs(&self) -> &[String] {
        &self.artifacts.declaration.verbs
    }

    pub fn scheme(&self) -> &Ste100Scheme {
        self.artifacts.scheme()
    }

    pub fn manifest(&self) -> &Ste100ModelManifest {
        self.artifacts.manifest()
    }

    pub(super) fn identity(&self) -> AdmissionIdentity {
        AdmissionIdentity {
            bootstrap: self.bootstrap().clone(),
            ready: self.ready.clone(),
            verbs: self.verbs().to_vec(),
        }
    }

    pub fn into_backend(self) -> RetainedBackend<B> {
        self.backend
    }
}

impl<B> Admitted<B> {
    pub(super) fn check(&mut self, request: &Ste100Request) -> Ste100Result
    where
        B: CheckBackend,
    {
        self.backend.check(request)
    }
}

/// Validate a complete bootstrap against the trusted first-receive limit.
///
/// Transport checks the declared total before allocation. This second check makes
/// the opaque handoff enforce the same bound before JSON decoding. Declared-path
/// admission consumes that handoff instead of decoding the bootstrap again.
pub fn validate_bootstrap(
    launch_limit: DecimalU64,
    bytes: &[u8],
) -> Result<ValidatedBootstrap, Ste100AdmissionRefused> {
    let limit = usize::try_from(launch_limit.0)
        .ok()
        .filter(|limit| *limit > 0)
        .ok_or_else(|| {
            refused(
                Ste100AdmissionCode::InvalidInstruction,
                "invalid launch limit",
            )
        })?;
    if bytes.len() > limit {
        return Err(refused(
            Ste100AdmissionCode::InvalidInstruction,
            "bootstrap exceeds the trusted launch limit",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        refused(
            Ste100AdmissionCode::InvalidInstruction,
            "bootstrap is not UTF-8 JSON",
        )
    })?;
    let bootstrap: Ste100Bootstrap = decode_strict(text).map_err(|error| {
        refused(
            Ste100AdmissionCode::InvalidInstruction,
            format!("invalid bootstrap: {error}"),
        )
    })?;
    bootstrap
        .validate(launch_limit)
        .map_err(|error| refused(Ste100AdmissionCode::InvalidInstruction, error.to_string()))?;
    Ok(ValidatedBootstrap { bootstrap })
}

/// Exercise a production adapter with supplied bytes in test builds only.
///
/// No production adapter factory is elected here. Unsupported grammar or model
/// formats remain admission refusals, never a fallback to synthetic checking.
#[cfg(test)]
pub(super) fn admit<A: AdapterAdmission>(
    launch_limit: DecimalU64,
    bootstrap_bytes: &[u8],
    supplied: SuppliedArtifacts<'_>,
    adapter: &A,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    if adapter.kind() != AdapterKind::Production {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "a test adapter cannot produce production readiness",
        ));
    }
    admit_checked(launch_limit, bootstrap_bytes, supplied, adapter, false)
}

/// This constructor is absent from production builds and never selected by config.
#[cfg(test)]
pub(super) fn admit_fixture<A: AdapterAdmission>(
    launch_limit: DecimalU64,
    bootstrap_bytes: &[u8],
    supplied: SuppliedArtifacts<'_>,
    adapter: &A,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    if adapter.kind() != AdapterKind::TestOnly {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "the fixture constructor requires a test-only adapter",
        ));
    }
    admit_checked(launch_limit, bootstrap_bytes, supplied, adapter, true)
}

#[cfg(test)]
fn admit_checked<A: AdapterAdmission>(
    launch_limit: DecimalU64,
    bootstrap_bytes: &[u8],
    supplied: SuppliedArtifacts<'_>,
    adapter: &A,
    fixture: bool,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    let bootstrap = validate_bootstrap(launch_limit, bootstrap_bytes)?;
    let scheme = verify_scheme(bootstrap.bootstrap, fixture)?;
    adapter.precheck().map_err(checked_adapter_refusal)?;
    let manifest_bytes = Arc::new(supplied.manifest.to_vec());
    let manifest = verify_manifest(&scheme.bootstrap.instruction, &manifest_bytes)?;
    verify_compatibility(&scheme, &manifest, fixture)?;
    let files = verify_files(&manifest, supplied.files)?;
    let artifacts = scheme.with_manifest(manifest);
    finish_admission(artifacts, files, manifest_bytes, adapter)
}

/// Acquire the exact declared manifest and every confined named input as the
/// current process identity. No compiled adapter is selected by this helper.
/// The supplied-byte route is absent from production:
///
/// ```compile_fail
/// use weaver_spu::ste100::admission::{admit, SuppliedArtifacts};
/// ```
/// Acquisition alone never produces Ready, and fixture adapters stay forbidden.
pub fn admit_from_path<A: AdapterAdmission>(
    bootstrap: ValidatedBootstrap,
    adapter: &A,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    if adapter.kind() != AdapterKind::Production {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "a test adapter cannot produce production readiness",
        ));
    }
    admit_validated_path(bootstrap, adapter, false)
}

#[cfg(test)]
pub(super) fn admit_from_path_fixture<A: AdapterAdmission>(
    bootstrap: ValidatedBootstrap,
    adapter: &A,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    if adapter.kind() != AdapterKind::TestOnly {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "the fixture constructor requires a test-only adapter",
        ));
    }
    admit_validated_path(bootstrap, adapter, true)
}

fn admit_validated_path<A: AdapterAdmission>(
    bootstrap: ValidatedBootstrap,
    adapter: &A,
    fixture: bool,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    let scheme = verify_scheme(bootstrap.bootstrap, fixture)?;
    adapter.precheck().map_err(checked_adapter_refusal)?;
    // The opaque owner checked this exact immutable instruction. Cheap scheme
    // checks precede filesystem access; compatibility precedes named input reads.
    let acquired = super::artifacts::AcquiredArtifacts::acquire_compatible(
        &scheme.bootstrap.instruction,
        |manifest| verify_compatibility(&scheme, manifest, fixture),
    )?;
    let artifacts = scheme.with_manifest(acquired.manifest);
    finish_admission(artifacts, acquired.files, acquired.manifest_bytes, adapter)
}

#[cfg(test)]
pub(super) fn admit_acquired_fixture<A: AdapterAdmission>(
    bootstrap: Ste100Bootstrap,
    acquired: super::artifacts::AcquiredArtifacts,
    adapter: &A,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    if adapter.kind() != AdapterKind::TestOnly {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "the fixture constructor requires a test-only adapter",
        ));
    }
    // A fixture may pass a separately acquired owner, so bind it to this exact
    // instruction without reparsing or rehashing the owner's immutable manifest.
    if acquired.instruction != bootstrap.instruction {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "acquired manifest belongs to a different instruction",
        ));
    }
    let scheme = verify_scheme(bootstrap, true)?;
    adapter.precheck().map_err(checked_adapter_refusal)?;
    verify_compatibility(&scheme, &acquired.manifest, true)?;
    let artifacts = scheme.with_manifest(acquired.manifest);
    finish_admission(artifacts, acquired.files, acquired.manifest_bytes, adapter)
}

fn checked_adapter_refusal(error: Ste100AdmissionRefused) -> Ste100AdmissionRefused {
    if error.validate().is_ok() {
        error
    } else {
        refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "compiled adapter returned an invalid admission refusal",
        )
    }
}

fn finish_admission<A: AdapterAdmission>(
    artifacts: VerifiedArtifacts,
    files: VerifiedFiles,
    manifest_bytes: Arc<Vec<u8>>,
    adapter: &A,
) -> Result<Admitted<A::Backend>, Ste100AdmissionRefused> {
    let backend = adapter
        .admit(&artifacts, &files)
        .map_err(checked_adapter_refusal)?;
    let ready = Ste100Ready {
        protocol: STE100_PROTOCOL.into(),
        scheme_id: artifacts.declaration.scheme_id.clone(),
        artifact_sha256: artifacts.declaration.artifact_sha256.clone(),
        checker_artifact_sha256: artifacts
            .bootstrap
            .instruction
            .model_artifact_sha256
            .clone(),
        instances: artifacts.bootstrap.instruction.instances,
        max_wire_bytes: artifacts.bootstrap.instruction.max_wire_bytes,
    };
    Ok(Admitted {
        artifacts,
        ready,
        backend: RetainedBackend {
            backend,
            files,
            manifest: manifest_bytes,
        },
    })
}

// Private cheap-input phase; no caller can construct a verified owner directly.
struct VerifiedScheme {
    bootstrap: Ste100Bootstrap,
    scheme: Ste100Scheme,
    declaration: SchemeDeclaration,
    grammar: Vec<u8>,
    conditions: Vec<u8>,
}

impl VerifiedScheme {
    fn with_manifest(self, manifest: Ste100ModelManifest) -> VerifiedArtifacts {
        VerifiedArtifacts {
            bootstrap: self.bootstrap,
            scheme: self.scheme,
            declaration: self.declaration,
            manifest,
            grammar: self.grammar,
            conditions: self.conditions,
            #[cfg(test)]
            grammar_lifetime: Arc::new(()),
        }
    }
}

fn verify_scheme(
    bootstrap: Ste100Bootstrap,
    fixture: bool,
) -> Result<VerifiedScheme, Ste100AdmissionRefused> {
    let scheme_hash = sha256(bootstrap.scheme_text.as_bytes());
    let scheme: Ste100Scheme = decode_strict(&bootstrap.scheme_text).map_err(|error| {
        refused(
            Ste100AdmissionCode::InvalidGrammar,
            format!("invalid scheme: {error}"),
        )
    })?;
    // Bootstrap carries the exact scheme bytes, not a second declaration. The
    // caller checks this content-derived identity against inventory on Ready.
    let declaration = SchemeDeclaration {
        scheme_id: format!("ste100:{}", scheme_hash.as_str()),
        artifact_sha256: scheme_hash,
        verbs: scheme.verbs.iter().map(|verb| verb.key.clone()).collect(),
    };
    let (grammar, conditions) = scheme
        .validate_and_decode(&declaration.artifact_sha256, &declaration)
        .map_err(|error| refused(Ste100AdmissionCode::InvalidGrammar, error.to_string()))?;
    scheme
        .verify_decoded_blobs(&grammar, &conditions, sha256)
        .map_err(|error| refused(Ste100AdmissionCode::InvalidGrammar, error.to_string()))?;
    if !fixture
        && [
            scheme.reference.edition.as_str(),
            scheme.grammar.format.as_str(),
            scheme.conditions.format.as_str(),
        ]
        .contains(&"fixture-read/1")
    {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "fixture-read/1 cannot be admitted by a production adapter",
        ));
    }

    Ok(VerifiedScheme {
        bootstrap,
        scheme,
        declaration,
        grammar,
        conditions,
    })
}

fn verify_compatibility(
    scheme: &VerifiedScheme,
    manifest: &Ste100ModelManifest,
    fixture: bool,
) -> Result<(), Ste100AdmissionRefused> {
    if scheme.scheme.qualification.checker_abi != manifest.checker_abi {
        return Err(refused(
            Ste100AdmissionCode::SchemeMismatch,
            "scheme and model checker ABIs differ",
        ));
    }
    if !fixture
        && [
            manifest.encoder.family.as_str(),
            manifest.alignment.as_str(),
        ]
        .contains(&"fixture-read/1")
    {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            "fixture-read/1 cannot be admitted by a production adapter",
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn verify_manifest(
    instruction: &Ste100Instruction,
    bytes: &[u8],
) -> Result<Ste100ModelManifest, Ste100AdmissionRefused> {
    if sha256(bytes) != instruction.model_artifact_sha256 {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            format!(
                "manifest {}: digest differs from the loaded instruction",
                instruction.model_binding.artifact.0
            ),
        ));
    }
    parse_manifest(instruction, bytes)
}

pub(super) fn parse_manifest(
    instruction: &Ste100Instruction,
    bytes: &[u8],
) -> Result<Ste100ModelManifest, Ste100AdmissionRefused> {
    let fail = |detail: String| {
        refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            format!(
                "manifest {}: {detail}",
                instruction.model_binding.artifact.0
            ),
        )
    };
    let text = std::str::from_utf8(bytes).map_err(|_| fail("not UTF-8 JSON".into()))?;
    let manifest: Ste100ModelManifest =
        decode_strict(text).map_err(|error| fail(format!("invalid model manifest: {error}")))?;
    manifest
        .validate()
        .map_err(|error| fail(error.to_string()))?;
    Ok(manifest)
}

#[cfg(test)]
fn verify_files(
    manifest: &Ste100ModelManifest,
    supplied: &[ArtifactBytes<'_>],
) -> Result<VerifiedFiles, Ste100AdmissionRefused> {
    let mut inventory = BTreeMap::new();
    for file in supplied {
        if inventory.insert(file.name, file.bytes).is_some() {
            return Err(refused(
                Ste100AdmissionCode::UnsupportedArtifact,
                "duplicate name in supplied artifact inventory",
            ));
        }
    }
    let mut files = BTreeMap::new();
    for reference in std::iter::once(&manifest.encoder.config)
        .chain(manifest.encoder.weights.iter())
        .chain([&manifest.tokenizer, &manifest.span_head])
    {
        verify_file(reference, &inventory, &mut files)?;
    }
    Ok(VerifiedFiles { files })
}

#[cfg(test)]
fn verify_file(
    reference: &ArtifactFile,
    inventory: &BTreeMap<&str, &[u8]>,
    files: &mut BTreeMap<String, Arc<Vec<u8>>>,
) -> Result<(), Ste100AdmissionRefused> {
    if !files.contains_key(&reference.name) {
        let bytes = inventory.get(reference.name.as_str()).ok_or_else(|| {
            refused(
                Ste100AdmissionCode::UnsupportedArtifact,
                format!("manifest file is absent: {}", reference.name),
            )
        })?;
        files.insert(reference.name.clone(), Arc::new(bytes.to_vec()));
    }
    // Hash the retained owner, including every digest of a repeated reference.
    verify_file_digest(reference, &files[&reference.name])
}

#[cfg(test)]
fn verify_file_digest(
    reference: &ArtifactFile,
    bytes: &[u8],
) -> Result<(), Ste100AdmissionRefused> {
    if sha256(bytes) != reference.sha256 {
        return Err(refused(
            Ste100AdmissionCode::UnsupportedArtifact,
            format!("manifest file digest differs: {}", reference.name),
        ));
    }
    Ok(())
}

pub(super) fn sha256(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 lower hex is a valid Digest")
}

fn refused(code: Ste100AdmissionCode, detail: impl Into<String>) -> Ste100AdmissionRefused {
    Ste100AdmissionRefused {
        protocol: STE100_PROTOCOL.into(),
        code,
        detail: detail.into(),
    }
}

#[cfg(test)]
pub(super) fn fixture_admitted() -> (Ste100Bootstrap, Admitted<()>) {
    tests::fixture_admitted()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use weaver_types::state_flow::{
        EncoderArtifact, GrammarBlob, MeasuredWait, RepairCap, RuleDisposition, RuleReference,
        STE100_BINDER_ABI, STE100_LABELS, SchemeQualification, SchemeReference, Ste100Instruction,
        TechnicalNameDefinition, VerbDefinition,
    };
    use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

    const LIMIT: DecimalU64 = DecimalU64(65_536);

    struct Fixture {
        bootstrap: Ste100Bootstrap,
        manifest: Vec<u8>,
        scheme: Ste100Scheme,
        files: Vec<ArtifactBytes<'static>>,
    }

    impl Fixture {
        fn new() -> Self {
            let authority = RuleReference {
                rule_id: "fixture-rule".into(),
                disposition: RuleDisposition::TechnicalUse,
            };
            let scheme = Ste100Scheme {
                format: "ste100-scheme/1".into(),
                reference: SchemeReference {
                    edition: "fixture-read/1".into(),
                    document_sha256: sha256(b"reference"),
                    rule_ids: vec!["fixture-rule".into()],
                    dictionary_sha256: sha256(b"dictionary"),
                },
                grammar: GrammarBlob {
                    format: "fixture-read/1".into(),
                    bytes_base64: "YQ==".into(),
                    sha256: sha256(b"a"),
                },
                verbs: vec![VerbDefinition {
                    key: "read".into(),
                    forms: vec!["Read".into(), "read".into()],
                    sense: "test-only".into(),
                    authority: authority.clone(),
                }],
                technical_names: vec![TechnicalNameDefinition {
                    text: "item".into(),
                    authority,
                }],
                conditions: GrammarBlob {
                    format: "fixture-read/1".into(),
                    bytes_base64: "Yg==".into(),
                    sha256: sha256(b"b"),
                },
                recall_keys: vec!["read".into()],
                qualification: SchemeQualification {
                    report_sha256: sha256(b"unqualified fixture report"),
                    corpus_sha256: sha256(b"fixture corpus"),
                    checker_abi: STE100_BINDER_ABI.into(),
                },
            };
            let files = vec![
                ArtifactBytes {
                    name: "config.json",
                    bytes: b"config",
                },
                ArtifactBytes {
                    name: "weights/one",
                    bytes: b"weights one",
                },
                ArtifactBytes {
                    name: "weights/two",
                    bytes: b"weights two",
                },
                ArtifactBytes {
                    name: "tokenizer.json",
                    bytes: b"tokenizer",
                },
                ArtifactBytes {
                    name: "head",
                    bytes: b"span head",
                },
            ];
            let reference = |index: usize| ArtifactFile {
                name: files[index].name.into(),
                sha256: sha256(files[index].bytes),
            };
            let manifest = serde_json::to_vec(&Ste100ModelManifest {
                format: "ste100-model/1".into(),
                checker_abi: STE100_BINDER_ABI.into(),
                encoder: EncoderArtifact {
                    family: "fixture-read/1".into(),
                    config: reference(0),
                    weights: vec![reference(1), reference(2)],
                },
                tokenizer: reference(3),
                span_head: reference(4),
                alignment: "fixture-read/1".into(),
                max_sentence_tokens: 32,
                labels: STE100_LABELS.map(String::from).to_vec(),
            })
            .unwrap();
            let bootstrap = Ste100Bootstrap {
                protocol: STE100_PROTOCOL.into(),
                scheme_text: serde_json::to_string(&scheme).unwrap(),
                instruction: Ste100Instruction {
                    model_binding: ModelBinding {
                        artifact: ArtifactRef("/synthetic/supplied-bytes-only.json".into()),
                        devices: vec![DeviceOrdinal(0)],
                    },
                    model_artifact_sha256: sha256(&manifest),
                    instances: 2,
                    max_wire_bytes: LIMIT,
                    measurement_ref: "fixture-only".into(),
                    repair_cap: RepairCap {
                        attempts: 2,
                        measurement_ref: "fixture-only".into(),
                    },
                    decision_wait: MeasuredWait {
                        elapsed_ns: DecimalU64(1),
                        measurement_ref: "fixture-only".into(),
                    },
                },
            };
            Self {
                bootstrap,
                manifest,
                scheme,
                files,
            }
        }

        fn refresh_scheme(&mut self) {
            self.bootstrap.scheme_text = serde_json::to_string(&self.scheme).unwrap();
        }

        fn remove_fixture_markers(&mut self) {
            self.scheme.reference.edition = "unqualified candidate".into();
            self.scheme.grammar.format = "unknown-grammar".into();
            self.scheme.conditions.format = "unknown-conditions".into();
            self.refresh_scheme();
            let mut manifest: Ste100ModelManifest =
                decode_strict(std::str::from_utf8(&self.manifest).unwrap()).unwrap();
            manifest.encoder.family = "unknown-family".into();
            manifest.alignment = "unknown-alignment".into();
            self.manifest = serde_json::to_vec(&manifest).unwrap();
            self.bootstrap.instruction.model_artifact_sha256 = sha256(&self.manifest);
        }

        fn artifacts(&self) -> SuppliedArtifacts<'_> {
            SuppliedArtifacts {
                manifest: &self.manifest,
                files: &self.files,
            }
        }

        fn bytes(&self) -> Vec<u8> {
            serde_json::to_vec(&self.bootstrap).unwrap()
        }

        fn check(&self) -> Result<Admitted<()>, Ste100AdmissionRefused> {
            admit_fixture(
                LIMIT,
                &self.bytes(),
                self.artifacts(),
                &FixtureAdapter::default(),
            )
        }
    }

    pub(super) fn fixture_admitted() -> (Ste100Bootstrap, Admitted<()>) {
        let fixture = Fixture::new();
        let admitted = fixture.check().unwrap();
        (fixture.bootstrap, admitted)
    }

    #[derive(Default)]
    struct FixtureAdapter {
        calls: Cell<usize>,
    }
    impl SealedAdapter for FixtureAdapter {}
    impl AdapterAdmission for FixtureAdapter {
        type Backend = ();
        fn kind(&self) -> AdapterKind {
            AdapterKind::TestOnly
        }
        fn admit(
            &self,
            artifacts: &VerifiedArtifacts,
            files: &VerifiedFiles,
        ) -> Result<(), Ste100AdmissionRefused> {
            self.calls.set(self.calls.get() + 1);
            assert_eq!(artifacts.grammar(), b"a");
            assert_eq!(artifacts.conditions(), b"b");
            assert_eq!(files.get("head"), Some(b"span head".as_slice()));
            assert_eq!(files.get("unreferenced"), None);
            Ok(())
        }
    }

    #[test]
    fn readiness_binds_exact_scheme_manifest_files_and_declared_instances() {
        let mut fixture = Fixture::new();
        fixture.files.push(ArtifactBytes {
            name: "unreferenced",
            bytes: b"not a model file",
        });
        let admitted = fixture.check().unwrap();
        admitted
            .ready()
            .validate_for(
                &admitted.artifacts.declaration,
                &fixture.bootstrap.instruction,
            )
            .unwrap();
        assert_eq!(admitted.verbs(), &[String::from("read")]);
        assert_eq!(
            admitted.ready().artifact_sha256,
            sha256(fixture.bootstrap.scheme_text.as_bytes())
        );
        assert_eq!(
            admitted.ready().checker_artifact_sha256,
            sha256(&fixture.manifest)
        );
        assert_eq!(admitted.ready().instances, 2);
        assert_eq!(admitted.ready().max_wire_bytes, LIMIT);
    }

    #[test]
    fn production_entry_rejects_the_explicit_synthetic_adapter_without_loading_it() {
        let mut fixture = Fixture::new();
        // Isolate adapter custody from the separate fixture-artifact guard.
        fixture.remove_fixture_markers();
        let adapter = FixtureAdapter::default();
        let error = admit(LIMIT, &fixture.bytes(), fixture.artifacts(), &adapter).unwrap_err();
        assert_eq!(error.code, Ste100AdmissionCode::UnsupportedArtifact);
        assert_eq!(adapter.calls.get(), 0);
        assert!(admit_fixture(LIMIT, &fixture.bytes(), fixture.artifacts(), &adapter).is_ok());
        assert_eq!(adapter.calls.get(), 1);
    }

    #[test]
    fn launch_and_bootstrap_limits_and_nested_keys_are_checked_before_admission() {
        let fixture = Fixture::new();
        for limit in [DecimalU64(0), DecimalU64(1), DecimalU64(LIMIT.0 - 1)] {
            assert_eq!(
                validate_bootstrap(limit, &fixture.bytes())
                    .unwrap_err()
                    .code,
                Ste100AdmissionCode::InvalidInstruction
            );
        }
        let mut bytes = fixture.bytes();
        bytes.splice(1..1, b"\"protocol\":\"state-ste100/1\",".iter().copied());
        assert!(validate_bootstrap(LIMIT, &bytes).is_err());
        let mut value = serde_json::to_value(&fixture.bootstrap).unwrap();
        value["instruction"]["surprise"] = true.into();
        assert!(validate_bootstrap(LIMIT, &serde_json::to_vec(&value).unwrap()).is_err());
        value = serde_json::to_value(&fixture.bootstrap).unwrap();
        value["instruction"]["instances"] = 0.into();
        assert!(validate_bootstrap(LIMIT, &serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn complete_bootstrap_fits_its_exact_limit_but_not_one_extra_byte() {
        let mut fixture = Fixture::new();
        loop {
            let length = DecimalU64(fixture.bytes().len() as u64);
            if fixture.bootstrap.instruction.max_wire_bytes == length {
                break;
            }
            fixture.bootstrap.instruction.max_wire_bytes = length;
        }
        let limit = fixture.bootstrap.instruction.max_wire_bytes;
        let mut bytes = fixture.bytes();
        assert_eq!(bytes.len() as u64, limit.0);
        assert!(validate_bootstrap(limit, &bytes).is_ok());
        bytes.push(b' ');
        assert_eq!(
            validate_bootstrap(limit, &bytes).unwrap_err().code,
            Ste100AdmissionCode::InvalidInstruction
        );
    }

    #[test]
    fn scheme_whitespace_is_identity_and_each_executable_blob_is_hashed() {
        let mut fixture = Fixture::new();
        let initial = fixture.check().unwrap().ready().artifact_sha256.clone();
        fixture.bootstrap.scheme_text.push('\n');
        let changed = fixture.check().unwrap().ready().artifact_sha256.clone();
        assert_ne!(initial, changed);
        for field in ["grammar", "conditions"] {
            let mut fixture = Fixture::new();
            if field == "grammar" {
                fixture.scheme.grammar.sha256 = sha256(b"wrong");
            } else {
                fixture.scheme.conditions.sha256 = sha256(b"wrong");
            }
            fixture.refresh_scheme();
            assert_eq!(
                fixture.check().unwrap_err().code,
                Ste100AdmissionCode::InvalidGrammar
            );
        }
    }

    #[test]
    fn strict_scheme_dictionary_and_manifest_abi_do_not_admit_approximations() {
        let mut fixture = Fixture::new();
        fixture.scheme.verbs[0].forms.push("read".into());
        fixture.refresh_scheme();
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::InvalidGrammar
        );
        fixture = Fixture::new();
        fixture.scheme.qualification.checker_abi = "wrong-abi".into();
        fixture.refresh_scheme();
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::SchemeMismatch
        );
        fixture = Fixture::new();
        fixture.bootstrap.scheme_text = fixture.bootstrap.scheme_text.replacen(
            "\"edition\":",
            "\"edition\":\"duplicate\",\"edition\":",
            1,
        );
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::InvalidGrammar
        );
        fixture = Fixture::new();
        let mut manifest: Ste100ModelManifest =
            decode_strict(std::str::from_utf8(&fixture.manifest).unwrap()).unwrap();
        manifest.labels.swap(1, 2);
        fixture.manifest = serde_json::to_vec(&manifest).unwrap();
        fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&fixture.manifest);
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::UnsupportedArtifact
        );
    }

    #[test]
    fn manifest_and_every_named_file_must_match_their_exact_supplied_bytes() {
        let mut fixture = Fixture::new();
        fixture.manifest.push(b'\n');
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::UnsupportedArtifact
        );
        for index in 0..5 {
            let mut fixture = Fixture::new();
            fixture.files[index].bytes = b"substituted";
            assert_eq!(
                fixture.check().unwrap_err().code,
                Ste100AdmissionCode::UnsupportedArtifact
            );
            let mut fixture = Fixture::new();
            fixture.files.remove(index);
            assert_eq!(
                fixture.check().unwrap_err().code,
                Ste100AdmissionCode::UnsupportedArtifact
            );
        }
        let mut fixture = Fixture::new();
        fixture.files.push(fixture.files[0]);
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::UnsupportedArtifact
        );
    }

    #[test]
    fn repeated_manifest_reference_checks_every_digest_without_banning_shared_bytes() {
        let mut fixture = Fixture::new();
        let mut manifest: Ste100ModelManifest =
            decode_strict(std::str::from_utf8(&fixture.manifest).unwrap()).unwrap();
        manifest.tokenizer = manifest.encoder.config.clone();
        fixture.manifest = serde_json::to_vec(&manifest).unwrap();
        fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&fixture.manifest);
        assert!(fixture.check().is_ok());
        manifest.tokenizer.sha256 = sha256(b"different bytes at the same path");
        fixture.manifest = serde_json::to_vec(&manifest).unwrap();
        fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&fixture.manifest);
        assert_eq!(
            fixture.check().unwrap_err().code,
            Ste100AdmissionCode::UnsupportedArtifact
        );
    }

    #[test]
    fn closed_manifest_and_relative_names_are_checked_after_digest_verification() {
        for replacement in [
            "\"format\":\"ste100-model/1\",\"format\":",
            "\"extra\":true,\"format\":",
        ] {
            let mut fixture = Fixture::new();
            fixture.manifest = String::from_utf8(fixture.manifest)
                .unwrap()
                .replacen("\"format\":", replacement, 1)
                .into_bytes();
            fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&fixture.manifest);
            assert_eq!(
                fixture.check().unwrap_err().code,
                Ste100AdmissionCode::UnsupportedArtifact
            );
        }
        for name in ["../head", "/head", "dir//head", "dir/./head", "dir/../head"] {
            let mut fixture = Fixture::new();
            let mut manifest: Ste100ModelManifest =
                decode_strict(std::str::from_utf8(&fixture.manifest).unwrap()).unwrap();
            manifest.span_head.name = name.into();
            fixture.manifest = serde_json::to_vec(&manifest).unwrap();
            fixture.bootstrap.instruction.model_artifact_sha256 = sha256(&fixture.manifest);
            assert_eq!(
                fixture.check().unwrap_err().code,
                Ste100AdmissionCode::UnsupportedArtifact
            );
        }
    }

    struct RejectingProduction {
        calls: Cell<usize>,
    }
    impl SealedAdapter for RejectingProduction {}
    impl AdapterAdmission for RejectingProduction {
        type Backend = ();
        fn kind(&self) -> AdapterKind {
            AdapterKind::Production
        }
        fn admit(
            &self,
            _: &VerifiedArtifacts,
            _: &VerifiedFiles,
        ) -> Result<(), Ste100AdmissionRefused> {
            self.calls.set(self.calls.get() + 1);
            Err(refused(
                Ste100AdmissionCode::UnsupportedArtifact,
                "no compiled adapter",
            ))
        }
    }

    #[test]
    fn fixture_artifact_cannot_reach_a_production_adapter_and_unsupported_stays_refused() {
        let mut fixture = Fixture::new();
        let adapter = RejectingProduction {
            calls: Cell::new(0),
        };
        assert_eq!(
            admit(LIMIT, &fixture.bytes(), fixture.artifacts(), &adapter)
                .unwrap_err()
                .code,
            Ste100AdmissionCode::UnsupportedArtifact
        );
        assert_eq!(adapter.calls.get(), 0);
        fixture.remove_fixture_markers();
        let error = admit(LIMIT, &fixture.bytes(), fixture.artifacts(), &adapter).unwrap_err();
        assert_eq!(error.code, Ste100AdmissionCode::UnsupportedArtifact);
        assert_eq!(adapter.calls.get(), 1);
    }
}
