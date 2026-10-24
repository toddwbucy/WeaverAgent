//! Adapter-owned precheck refusals at every fixture admission boundary.
use super::*;

struct RefusePrecheck {
    calls: Cell<usize>,
    invalid: bool,
}
impl SealedAdapter for RefusePrecheck {}
impl AdapterAdmission for RefusePrecheck {
    type Backend = ();
    fn kind(&self) -> AdapterKind {
        AdapterKind::TestOnly
    }
    fn precheck(&self) -> Result<(), Ste100AdmissionRefused> {
        self.calls.set(self.calls.get() + 1);
        Err(Ste100AdmissionRefused {
            protocol: if self.invalid {
                "invalid/0"
            } else {
                STE100_PROTOCOL
            }
            .into(),
            code: Ste100AdmissionCode::UnsupportedArtifact,
            detail: "this exact compiled fixture adapter is unavailable".into(),
        })
    }
    fn admit(
        &self,
        _: &VerifiedArtifacts,
        _: &VerifiedFiles,
    ) -> Result<(), Ste100AdmissionRefused> {
        panic!("a refused precheck must never reach artifact admission")
    }
}

fn through_route(
    route: &str,
    fixture: &Fixture,
    adapter: &RefusePrecheck,
    invalid_scheme: bool,
) -> Result<Admitted<()>, Ste100AdmissionRefused> {
    // Acquisition here is the genuine pre-acquired test route's responsibility.
    // It is deliberately retained before changing the scheme, not manufactured.
    let acquired = (route == "acquired").then(|| fixture.acquire().unwrap());
    let mut bootstrap = fixture.bootstrap.clone();
    if invalid_scheme {
        bootstrap.scheme_text = "{}".into();
    }
    let bytes = serde_json::to_vec(&bootstrap).unwrap();
    match route {
        "path" => {
            // Exact refusal must precede trying to read this now-missing manifest.
            std::fs::remove_file(fixture.path()).unwrap();
            admission::admit_from_path_fixture(
                admission::validate_bootstrap(bootstrap.instruction.max_wire_bytes, &bytes)?,
                adapter,
            )
        }
        "supplied" => admission::admit_fixture(
            bootstrap.instruction.max_wire_bytes,
            &bytes,
            admission::SuppliedArtifacts {
                manifest: b"not a manifest; precheck must refuse before parsing",
                files: &[],
            },
            adapter,
        ),
        "acquired" => admit_acquired_fixture(bootstrap, acquired.unwrap(), adapter),
        _ => unreachable!(),
    }
}

#[test]
fn adapter_precheck_review_retains_exact_refusal_and_validates_it_on_every_route() {
    for route in ["path", "supplied", "acquired"] {
        for invalid in [false, true] {
            let fixture = Fixture::new();
            let adapter = RefusePrecheck {
                calls: Cell::new(0),
                invalid,
            };
            let refusal = through_route(route, &fixture, &adapter, false).unwrap_err();
            assert_eq!(adapter.calls.get(), 1, "{route}");
            assert_eq!(refusal.protocol, STE100_PROTOCOL);
            assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
            assert_eq!(
                refusal.detail,
                if invalid {
                    "compiled adapter returned an invalid admission refusal"
                } else {
                    "this exact compiled fixture adapter is unavailable"
                },
                "{route}"
            );
            assert!(refusal.validate().is_ok());
        }
    }
}

#[test]
fn adapter_precheck_review_keeps_cheap_scheme_failure_before_adapter_refusal() {
    for route in ["path", "supplied", "acquired"] {
        let fixture = Fixture::new();
        let adapter = RefusePrecheck {
            calls: Cell::new(0),
            invalid: false,
        };
        let refusal = through_route(route, &fixture, &adapter, true).unwrap_err();
        assert_eq!(refusal.code, Ste100AdmissionCode::InvalidGrammar, "{route}");
        assert_eq!(adapter.calls.get(), 0, "{route}");
    }
}
