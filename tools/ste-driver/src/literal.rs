//! Workflow 6.02. Match ste_experiment's literal check at 9881da6.

use crate::python_re::compile;
use fancy_regex::Regex;

/// Check attachment to placeholders separately from their occurrence counts.
pub struct LiteralCheck {
    glued: Regex,
    placeholder: Regex,
    slashed: Regex,
}

impl LiteralCheck {
    pub fn new() -> Result<Self, fancy_regex::Error> {
        Ok(Self {
            glued: compile(r"\S*\[\[C\d+\]\]\S*")?,
            placeholder: compile(r"\A\[\[C\d+\]\]\z")?,
            slashed: compile(r"\A\[\[C\d+\]\](?:/\[\[C\d+\]\])+\z")?,
        })
    }

    /// Keep each source token in source order, including repeated tokens.
    pub fn glued<'a>(&self, source: &'a str) -> Result<Vec<&'a str>, fancy_regex::Error> {
        let mut out = Vec::new();
        for matched in self.glued.find_iter(source) {
            let token = matched?
                .as_str()
                .trim_matches(|ch| "*_.,:\x3b!?()\"'".contains(ch));
            if !token.is_empty()
                && !self.placeholder.is_match(token)?
                && !self.slashed.is_match(token)?
            {
                out.push(token);
            }
        }
        Ok(out)
    }

    /// Missing output changes every glued literal. No output is repaired here.
    pub fn altered<'a>(
        &self,
        output: Option<&str>,
        source: &'a str,
    ) -> Result<Vec<&'a str>, fancy_regex::Error> {
        let output = output.unwrap_or_default();
        Ok(self
            .glued(source)?
            .into_iter()
            .filter(|t| !output.contains(t))
            .collect())
    }
}
