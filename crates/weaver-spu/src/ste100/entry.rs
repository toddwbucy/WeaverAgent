//! Exact launch and inherited-fd boundary of the separate STE100 duty.
use std::ffi::OsString;
use std::process::ExitCode;

use weaver_types::state_flow::{
    DecimalU64, Ste100AdmissionCode, Ste100AdmissionRefused, Ste100Request, Ste100Result,
};

use super::admission::{
    AdapterAdmission, AdapterKind, CheckBackend, SealedAdapter, VerifiedArtifacts, VerifiedFiles,
};
use super::channel::Channel;
use super::dispatcher::Stop;
use super::runtime::{Runtime, RuntimeProgress};

fn limit(args: impl IntoIterator<Item = OsString>) -> Result<usize, &'static str> {
    let mut args = args.into_iter();
    let (Some(flag), Some(value), None) = (args.next(), args.next(), args.next()) else {
        return Err("expected --max-wire-bytes <positive-decimal>");
    };
    if flag != "--max-wire-bytes" {
        return Err("expected --max-wire-bytes <positive-decimal>");
    }
    let value = value.to_str().ok_or("launch limit is not UTF-8 decimal")?;
    let DecimalU64(value) = value
        .parse()
        .map_err(|_| "launch limit is not canonical unsigned decimal")?;
    usize::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or("launch limit is not a positive process-sized decimal")
}

// This is an explicit absence of a compiled adapter, not a permissive factory.
// No command-line/config field can select a fixture or the old label scorer.
struct NoCompiledAdapter;
impl SealedAdapter for NoCompiledAdapter {}
enum UnavailableBackend {}
impl AdapterAdmission for NoCompiledAdapter {
    type Backend = UnavailableBackend;
    fn kind(&self) -> AdapterKind {
        AdapterKind::Production
    }
    fn is_available(&self) -> bool {
        false
    }
    fn admit(
        &self,
        _: &VerifiedArtifacts,
        _: &VerifiedFiles,
    ) -> Result<Self::Backend, Ste100AdmissionRefused> {
        Err(Ste100AdmissionRefused {
            protocol: weaver_types::state_flow::STE100_PROTOCOL.into(),
            code: Ste100AdmissionCode::UnsupportedArtifact,
            detail: "no compatible STE100 adapter is compiled into this executable".into(),
        })
    }
}
impl CheckBackend for UnavailableBackend {
    fn check(&mut self, _: &Ste100Request) -> Ste100Result {
        match *self {}
    }
}

/// Run only the separate STE100 executable. The parent remains responsible for
/// its existing privilege, stderr, lifetime, timeout and reap choreography.
pub fn run() -> ExitCode {
    let limit = match limit(std::env::args_os().skip(1)) {
        Ok(limit) => limit,
        Err(error) => {
            eprintln!("STE100 launch: {error}");
            return ExitCode::FAILURE;
        }
    };
    let fd = match crate::channel::adopt_ste100() {
        Ok(fd) => fd,
        Err(error) => {
            eprintln!("STE100 boundary: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    let channel = match Channel::from_owned(fd, limit) {
        Ok(channel) => channel,
        Err(error) => {
            eprintln!("STE100 boundary: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    drive(Runtime::new(channel, NoCompiledAdapter))
}

fn drive<A>(mut runtime: Runtime<A>) -> ExitCode
where
    A: AdapterAdmission + Send + 'static,
    A::Backend: CheckBackend,
{
    loop {
        match runtime.advance() {
            Ok(RuntimeProgress::Progress) => {}
            Ok(RuntimeProgress::Pending) => {
                if let Err(error) = runtime.wait() {
                    eprintln!("STE100 duty: {error}");
                    return ExitCode::FAILURE;
                }
            }
            Ok(RuntimeProgress::Stopped(Stop::PeerClosed)) => return ExitCode::SUCCESS,
            Ok(RuntimeProgress::Stopped(Stop::AdmissionRefused | Stop::ComponentFailed)) => {
                return ExitCode::FAILURE;
            }
            Err(error) => {
                eprintln!("STE100 duty: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
}

#[cfg(test)]
#[path = "entry_review_tests.rs"]
mod review_tests;
