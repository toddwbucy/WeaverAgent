//! Which SPU each agent's worker forks, per `weaver-admin-Spec` section 9.
//!
//! `spu-binary` is the SPU of every agent `agent-spu` does not name. The two
//! optional files choose another per agent: `spu-implementations` maps a key to
//! an absolute path, and `agent-spu` maps an allow-listed agent to a key. They
//! are read with every other value of the service configuration, before any
//! verb, and judged there, so a map that contradicts itself fails the
//! invocation rather than a load.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The state member's binary, a sibling of the worker's, whose name the stack
/// already holds, per section 9.
const STATE_MEMBER: &str = "weaver-state";

/// The per-agent choice, judged whole at read.
#[derive(Debug, Clone, Default)]
pub struct SpuChoice {
    implementations: BTreeMap<String, PathBuf>,
    agents: BTreeMap<String, String>,
}

/// One agent's SPU: the key where the map chose it, and the path forked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSpu {
    pub key: Option<String>,
    pub path: PathBuf,
}

impl SpuChoice {
    /// Reads and judges the two optional files against the allow-list and the
    /// other binaries the stack names.
    ///
    /// **Every failure is the operator's file contradicting itself**, so each
    /// is an error naming the file and the line, which fails the invocation as
    /// an unreadable configuration does: a line that is not two fields, a key
    /// outside lowercase letters, digits and hyphens, a relative path, a key or
    /// an agent named twice, an agent the allow-list does not hold, a key the
    /// implementations do not hold, or an SPU whose file name another binary of
    /// the stack already has, the stack being keyed by file name.
    pub fn read(
        implementations: Option<&str>,
        agents: Option<&str>,
        allow_list: &[String],
        default_spu: &Path,
        worker: &Path,
        gate: &Path,
    ) -> Result<Self, String> {
        let mut choice = SpuChoice::default();
        for (number, line) in lines(implementations) {
            let (key, path) = two_fields("spu-implementations", number, line)?;
            if key.is_empty()
                || !key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return Err(format!(
                    "spu-implementations line {number}: the key {key:?} is not lowercase letters, digits and hyphens"
                ));
            }
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(format!(
                    "spu-implementations line {number}: the path {} is not absolute",
                    path.display()
                ));
            }
            if choice
                .implementations
                .insert(key.to_string(), path)
                .is_some()
            {
                return Err(format!(
                    "spu-implementations line {number}: the key {key:?} is named twice"
                ));
            }
        }
        for (number, line) in lines(agents) {
            let (agent, key) = two_fields("agent-spu", number, line)?;
            if !allow_list.iter().any(|a| a == agent) {
                return Err(format!(
                    "agent-spu line {number}: the agent {agent:?} is not on the allow-list"
                ));
            }
            if !choice.implementations.contains_key(key) {
                return Err(format!(
                    "agent-spu line {number}: the key {key:?} is not in spu-implementations"
                ));
            }
            if choice
                .agents
                .insert(agent.to_string(), key.to_string())
                .is_some()
            {
                return Err(format!(
                    "agent-spu line {number}: the agent {agent:?} is named twice"
                ));
            }
        }
        // **Every name the stack records differs from every other**, the stack
        // being keyed by file name: the worker, the state member and the gate
        // pairwise, and each SPU from those three. SPUs need not differ among
        // themselves, one record carrying one agent's SPU.
        let fixed = [
            ("the worker", file_name(worker)),
            ("the state member", STATE_MEMBER.to_string()),
            ("the gate", file_name(gate)),
        ];
        for (i, (one, name)) in fixed.iter().enumerate() {
            for (other, other_name) in &fixed[i + 1..] {
                if name == other_name {
                    return Err(format!(
                        "{one} and {other} share the file name {name:?}, which the stack keys by"
                    ));
                }
            }
        }
        for spu in std::iter::once(default_spu)
            .chain(choice.implementations.values().map(PathBuf::as_path))
        {
            let name = file_name(spu);
            if let Some((other, _)) = fixed.iter().find(|(_, n)| *n == name) {
                return Err(format!(
                    "the SPU binary {} shares its file name with {other}, which the stack keys by",
                    spu.display()
                ));
            }
        }
        Ok(choice)
    }

    /// The SPU an agent's worker forks: the map's choice where it names the
    /// agent, and the installation's `spu-binary` where it does not.
    pub fn for_agent(&self, agent: &str, default_spu: &Path) -> AgentSpu {
        match self.agents.get(agent) {
            Some(key) => AgentSpu {
                key: Some(key.clone()),
                path: self.implementations[key].clone(),
            },
            None => AgentSpu {
                key: None,
                path: default_spu.to_path_buf(),
            },
        }
    }
}

fn lines(text: Option<&str>) -> impl Iterator<Item = (usize, &str)> {
    text.unwrap_or_default()
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty())
}

fn two_fields<'a>(file: &str, number: usize, line: &'a str) -> Result<(&'a str, &'a str), String> {
    let mut fields = line.split_whitespace();
    match (fields.next(), fields.next(), fields.next()) {
        (Some(a), Some(b), None) => Ok((a, b)),
        _ => Err(format!(
            "{file} line {number}: expected two fields, found {line:?}"
        )),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = "/opt/weaver/bin/weaver-spu";
    const WORKER: &str = "/opt/weaver/bin/worker";
    const GATE: &str = "/opt/weaver/bin/weaver-gate";

    fn read(implementations: Option<&str>, agents: Option<&str>) -> Result<SpuChoice, String> {
        read_with(implementations, agents, DEFAULT)
    }

    fn read_with(
        implementations: Option<&str>,
        agents: Option<&str>,
        default_spu: &str,
    ) -> Result<SpuChoice, String> {
        SpuChoice::read(
            implementations,
            agents,
            &["karl".to_string(), "ada".to_string()],
            Path::new(default_spu),
            Path::new(WORKER),
            Path::new(GATE),
        )
    }

    #[test]
    fn an_installation_without_the_map_serves_every_agent_the_default() {
        let choice = read(None, None).unwrap();
        for agent in ["karl", "ada"] {
            assert_eq!(
                choice.for_agent(agent, Path::new(DEFAULT)),
                AgentSpu {
                    key: None,
                    path: PathBuf::from(DEFAULT)
                }
            );
        }
    }

    #[test]
    fn a_named_agent_takes_its_key_and_the_rest_the_default() {
        let choice = read(
            Some(
                "python /opt/weaver/python-spu/python-spu.pyz\nrust /opt/weaver/bin/weaver-spu-b\n",
            ),
            Some("karl python\n"),
        )
        .unwrap();
        assert_eq!(
            choice.for_agent("karl", Path::new(DEFAULT)),
            AgentSpu {
                key: Some("python".into()),
                path: PathBuf::from("/opt/weaver/python-spu/python-spu.pyz"),
            }
        );
        assert_eq!(
            choice.for_agent("ada", Path::new(DEFAULT)),
            AgentSpu {
                key: None,
                path: PathBuf::from(DEFAULT)
            }
        );
    }

    #[test]
    fn blank_lines_are_not_entries() {
        let choice = read(
            Some("\npython /opt/p/python-spu.pyz\n\n"),
            Some("\n\nkarl python\n"),
        )
        .unwrap();
        assert_eq!(
            choice.for_agent("karl", Path::new(DEFAULT)).key.as_deref(),
            Some("python")
        );
    }

    /// Each of section 9's failures, each one asserted by its own message, so a
    /// case refused for the wrong reason fails.
    /// The fixed names differ pairwise, a gate named like the worker or like the
    /// state member overwriting a digest otherwise. Perturbation: check the SPUs
    /// alone, as the first form did, and both cases are accepted.
    #[test]
    fn the_worker_the_member_and_the_gate_differ_pairwise() {
        for gate in ["/opt/other/worker", "/opt/other/weaver-state"] {
            let failure = SpuChoice::read(
                None,
                None,
                &[],
                Path::new(DEFAULT),
                Path::new(WORKER),
                Path::new(gate),
            )
            .expect_err(gate);
            assert!(
                failure.contains("share the file name"),
                "{gate}: {failure:?}"
            );
        }
    }

    #[test]
    fn every_contradiction_fails_naming_itself() {
        let cases: &[(Option<&str>, Option<&str>, &str, &str)] = &[
            (Some("python"), None, DEFAULT, "expected two fields"),
            (Some("python /a /b"), None, DEFAULT, "expected two fields"),
            (
                Some("Python /opt/p/python-spu.pyz"),
                None,
                DEFAULT,
                "is not lowercase",
            ),
            (
                Some("py_thon /opt/p/python-spu.pyz"),
                None,
                DEFAULT,
                "is not lowercase",
            ),
            (
                Some("python opt/p/python-spu.pyz"),
                None,
                DEFAULT,
                "is not absolute",
            ),
            (
                Some("python /opt/p/a.pyz\npython /opt/p/b.pyz"),
                None,
                DEFAULT,
                "is named twice",
            ),
            (
                Some("python /opt/p/a.pyz"),
                Some("karl"),
                DEFAULT,
                "expected two fields",
            ),
            (
                Some("python /opt/p/a.pyz"),
                Some("eve python"),
                DEFAULT,
                "not on the allow-list",
            ),
            (
                Some("python /opt/p/a.pyz"),
                Some("karl rust"),
                DEFAULT,
                "not in spu-implementations",
            ),
            (
                Some("python /opt/p/a.pyz"),
                Some("karl python\nkarl python"),
                DEFAULT,
                "agent \"karl\" is named twice",
            ),
            (
                Some("python /opt/p/worker"),
                None,
                DEFAULT,
                "shares its file name",
            ),
            (
                Some("python /opt/p/weaver-gate"),
                None,
                DEFAULT,
                "shares its file name",
            ),
            (
                Some("python /opt/p/weaver-state"),
                None,
                DEFAULT,
                "shares its file name",
            ),
            (None, None, "/opt/other/worker", "shares its file name"),
            // `split_whitespace` is Rust's `char::is_whitespace`, which \x1c is
            // not, and `file_name` reads past a trailing separator: the matrix
            // mirrors both, so both are pinned here as there.
            (
                Some("python\u{1c}/opt/p/a.pyz"),
                None,
                DEFAULT,
                "expected two fields",
            ),
            (
                Some("python /opt/p/worker/"),
                None,
                DEFAULT,
                "shares its file name",
            ),
        ];
        for (implementations, agents, default_spu, message) in cases {
            let failure = read_with(*implementations, *agents, default_spu)
                .expect_err(&format!("{implementations:?} {agents:?} was accepted"));
            assert!(
                failure.contains(message),
                "{implementations:?} {agents:?}: {failure:?} does not say {message:?}"
            );
        }
    }
}
