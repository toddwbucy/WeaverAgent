//! Workflow 4.01, 4.05-4.07, 4.11, 6.01, 6.07 and 7.07 to 7.09, external entry.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use weaver_ste_driver::{
    chain,
    check::Checker,
    gate::{TurnClient, gate_call_deadline},
    guard::Judge,
    lexicon::Lexicon,
    prompt::{BUNDLE_SHA256, SYSTEM_SHA256, source_examples},
    run::{Kit, R0Parameters, evaluate_with_provenance, verify_folded_prompt},
    source::APPROVED_WORDS_SHA256,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const USAGE: &str = "weaver-ste-driver evaluate --config FILE --output DIRECTORY\n\
One frozen kit evaluation. All input paths and R0 caps are required in FILE.\n\
Exit 0: complete evaluation, 1: command failure, 2: void evaluation.\n\
A completed evaluation can include shape drops or language failures.";
const TRACE_USAGE: &str = "weaver-ste-driver trace-report --trace FILE --journal FILE --output DIRECTORY\n\
Offline trace evidence in a new directory. Exit 0: complete, 2: incomplete, 1: command failure.";
const PREPARE_USAGE: &str = "weaver-ste-driver prepare-kit --config FILE --output DIRECTORY\n\
Retain source and mask tables. Exit 0: prepared, 1: command failure. No row qualification.";
const SPLIT_USAGE: &str = "weaver-ste-driver split-kit --config FILE --output DIRECTORY\n\
Split a frozen kit between trajectories into a chain of batches in a new directory.\n\
Exit 0: split, 1: command failure.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareConfig {
    source: PathBuf,
    source_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SplitConfig {
    kit: PathBuf,
    kit_sha256: String,
    batch_trajectories: usize,
}

/// Workflow 7.09. A batch evaluation's place in its chain: the manifest, pinned by
/// its hash, and the batch's index in it.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChainConfig {
    manifest: PathBuf,
    manifest_sha256: String,
    index: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    repo: PathBuf,
    agent: String,
    config_root: PathBuf,
    train_root: PathBuf,
    approved_words: PathBuf,
    prompt_bundle: PathBuf,
    kit: PathBuf,
    kit_sha256: String,
    driver_repair_pass_cap: u32,
    offer_pass_cap: u32,
    transport_stop_bound: u64,
    gate_call_deadline_seconds: u64,
    /// Present only for a batch of a chain. A kit evaluated alone has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chain: Option<ChainConfig>,
}

impl Config {
    fn resolve(&mut self, parent: &Path) -> Result<()> {
        for path in [
            &mut self.repo,
            &mut self.config_root,
            &mut self.train_root,
            &mut self.approved_words,
            &mut self.prompt_bundle,
            &mut self.kit,
        ]
        .into_iter()
        .chain(self.chain.as_mut().map(|chain| &mut chain.manifest))
        {
            if path.as_os_str().is_empty() {
                return Err("an input path is empty".into());
            }
            if path.is_relative() {
                *path = parent.join(&*path);
            }
        }
        Ok(())
    }
}

enum Arguments {
    Help,
    Evaluate {
        config: PathBuf,
        output: PathBuf,
    },
    PrepareKit {
        config: PathBuf,
        output: PathBuf,
    },
    SplitKit {
        config: PathBuf,
        output: PathBuf,
    },
    TraceReport {
        trace: PathBuf,
        journal: PathBuf,
        output: PathBuf,
    },
}

fn arguments(args: impl Iterator<Item = OsString>) -> Result<Arguments> {
    let mut args = args;
    let first = args.next().ok_or(USAGE)?;
    if first == "--help" && args.next().is_none() {
        return Ok(Arguments::Help);
    }
    if first != "evaluate"
        && first != "trace-report"
        && first != "prepare-kit"
        && first != "split-kit"
    {
        return Err(USAGE.into());
    }
    let mut config = None;
    let mut output = None;
    let mut trace = None;
    let mut journal = None;
    while let Some(flag) = args.next() {
        let slot = if flag == "--config"
            && (first == "evaluate" || first == "prepare-kit" || first == "split-kit")
        {
            &mut config
        } else if flag == "--trace" && first == "trace-report" {
            &mut trace
        } else if flag == "--journal" && first == "trace-report" {
            &mut journal
        } else if flag == "--output" {
            &mut output
        } else {
            return Err("unknown command option".into());
        };
        if slot.is_some() {
            return Err("repeated command option".into());
        }
        let value = args.next().ok_or("missing command option value")?;
        if value.is_empty() || value.to_str().is_some_and(|s| s.starts_with("--")) {
            return Err("missing command option value".into());
        }
        *slot = Some(PathBuf::from(value));
    }
    if first == "trace-report" {
        return Ok(Arguments::TraceReport {
            trace: trace.ok_or("missing --trace")?,
            journal: journal.ok_or("missing --journal")?,
            output: output.ok_or("missing --output")?,
        });
    }
    if first == "prepare-kit" {
        return Ok(Arguments::PrepareKit {
            config: config.ok_or("missing --config")?,
            output: output.ok_or("missing --output")?,
        });
    }
    if first == "split-kit" {
        return Ok(Arguments::SplitKit {
            config: config.ok_or("missing --config")?,
            output: output.ok_or("missing --output")?,
        });
    }
    Ok(Arguments::Evaluate {
        config: config.ok_or("missing --config")?,
        output: output.ok_or("missing --output")?,
    })
}

/// Check pinned conversion inputs before creating a run or sending a gate request.
pub(crate) fn execute(args: impl Iterator<Item = OsString>) -> Result<u8> {
    match arguments(args)? {
        Arguments::Help => {
            println!("{USAGE}\n\n{TRACE_USAGE}\n\n{PREPARE_USAGE}\n\n{SPLIT_USAGE}");
            Ok(0)
        }
        Arguments::TraceReport {
            trace,
            journal,
            output,
        } => {
            let complete = weaver_ste_driver::trace_report::collect(&trace, &journal, &output)?;
            println!("{}", json!({"complete":complete,"output":output}));
            Ok(if complete { 0 } else { 2 })
        }
        Arguments::Evaluate { config, output } => evaluate(config, output),
        Arguments::SplitKit { config, output } => {
            let config_path = std::fs::canonicalize(config)?;
            let mut config: SplitConfig = serde_json::from_slice(&std::fs::read(&config_path)?)?;
            if config.kit.as_os_str().is_empty() {
                return Err("an input path is empty".into());
            }
            if config.kit.is_relative() {
                config.kit = config_path
                    .parent()
                    .ok_or("config path has no parent")?
                    .join(config.kit);
            }
            let (manifest, manifest_sha256) = chain::split(
                &config.kit,
                &config.kit_sha256,
                config.batch_trajectories,
                &output,
            )?;
            println!(
                "{}",
                json!({
                    "output":output, "manifest":chain::MANIFEST,
                    "manifest_sha256":manifest_sha256,
                    "kit_sha256":manifest.kit_sha256, "batches":manifest.batches.len(),
                    "segments":manifest.segments, "trajectories":manifest.trajectories
                })
            );
            Ok(0)
        }
        Arguments::PrepareKit { config, output } => {
            let config_path = std::fs::canonicalize(config)?;
            let mut config: PrepareConfig = serde_json::from_slice(&std::fs::read(&config_path)?)?;
            if config.source.as_os_str().is_empty() {
                return Err("source path is empty".into());
            }
            if config.source.is_relative() {
                config.source = config_path
                    .parent()
                    .ok_or("config path has no parent")?
                    .join(config.source);
            }
            let report = weaver_ste_driver::prepare::prepare(
                &config.source,
                &config.source_sha256,
                &output,
            )?;
            println!("{report}");
            Ok(0)
        }
    }
}

fn evaluate(config: PathBuf, output: PathBuf) -> Result<u8> {
    let config_path = std::fs::canonicalize(config)?;
    let bytes = std::fs::read(&config_path)?;
    let mut config: Config = serde_json::from_slice(&bytes)?;
    let parameters = R0Parameters::new(
        config.driver_repair_pass_cap,
        config.offer_pass_cap,
        config.transport_stop_bound,
    )?;
    let deadline = gate_call_deadline(config.gate_call_deadline_seconds)?;
    config.resolve(config_path.parent().ok_or("config path has no parent")?)?;
    let mut transport =
        TurnClient::new(&config.repo, &config.agent, &config.config_root, deadline)?;
    let kit = Kit::read(&config.kit)?;
    if config.kit_sha256 != kit.sha256() {
        return Err("frozen kit hash mismatch".into());
    }
    let chain_batch = match &config.chain {
        Some(chain) => Some(chain::check(
            &chain.manifest,
            &chain.manifest_sha256,
            chain.index,
            kit.sha256(),
        )?),
        None => None,
    };
    verify_folded_prompt(
        &config
            .repo
            .join("docs/crates/weaver-harness/Loops/ste-conversion-prompt.txt"),
    )?;
    let examples = source_examples(&config.prompt_bundle)?;
    let lexicon = Lexicon::load_pinned(&config.train_root, &config.approved_words)?;
    let judge = Judge::new(Checker::new(lexicon, 20, 25)?, &examples)?;
    let mut provenance = json!({
        "command":"evaluate", "config":config, "config_path":config_path,
        "config_sha256":format!("{:x}",Sha256::digest(&bytes)),
        "source_revision":"9881da6cfbc92b08b31264883aea63b73aa7b694",
        "source_system_sha256":SYSTEM_SHA256, "source_bundle_sha256":BUNDLE_SHA256,
        "approved_words_sha256":APPROVED_WORDS_SHA256,
        "live_agent_identity":"requires operator and trace verification"
    });
    if let Some(batch) = chain_batch {
        provenance["chain_batch"] = serde_json::to_value(batch)?;
    }
    let report = evaluate_with_provenance(
        &kit,
        &parameters,
        &judge,
        &mut transport,
        &output,
        &provenance,
    )?;
    println!("{}", report.to_json());
    Ok(if report.void { 2 } else { 0 })
}
