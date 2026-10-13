//! Test-only: the `parameter_tests` suite of `lifecycle.rs`,
//! moved out of the production file by #118.

use super::OrganParameters;

/// **A deployment that states nothing hands nothing**, so an organ keeps
/// the default it compiled and an installation that never thought about a
/// parameter behaves as it always did.
#[test]
fn no_parameter_is_no_argument() {
    assert!(OrganParameters::default().spu_arguments().is_empty());
}

/// A stated parameter travels as a named flag and its value, so a
/// deployment supplying one need not know the order of any other.
#[test]
fn a_stated_parameter_travels_named() {
    let parameters = OrganParameters {
        headroom_bytes: Some("268435456".to_string()),
    };
    assert_eq!(
        parameters.spu_arguments(),
        vec!["--headroom-bytes".to_string(), "268435456".to_string()]
    );
}
