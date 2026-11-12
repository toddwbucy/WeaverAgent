//! Workflow 6.01 and 4.11. Converter.guard and Judge at 9881da6.

use crate::check::{CheckResult, Checker, split_lines};
use crate::python_re::{compile, whitespace};
use fancy_regex::Regex;
use serde_json::json;
use std::collections::BTreeSet;

pub struct Judge {
    pub checker: Checker,
    example_lines: BTreeSet<String>,
    markup: Regex,
}

impl Judge {
    /// Supply the pinned examples' assistant texts in their source order.
    pub fn new(checker: Checker, examples: &[String]) -> Result<Self, fancy_regex::Error> {
        let example_lines = examples
            .iter()
            .flat_map(|s| split_lines(s))
            .map(|s| s.trim_matches(whitespace))
            .filter(|s| s.chars().count() >= 20)
            .map(str::to_owned)
            .collect();
        Ok(Self {
            checker,
            example_lines,
            markup: compile(r"</?[A-Za-z][\w-]*(?:\s[^<>\n]*)?/?>")?,
        })
    }

    pub fn check(
        &self,
        output: Option<&str>,
        source: &str,
        names: &BTreeSet<String>,
    ) -> Result<CheckResult, fancy_regex::Error> {
        let Some(output) = output else {
            let mut result = CheckResult::default();
            result.hard.insert("missing", vec![json!(true)]);
            return Ok(result);
        };
        let mut result = self.checker.check(output, source, names)?;
        let copied: Vec<_> = self
            .example_lines
            .iter()
            .filter(|s| output.contains(s.as_str()) && !source.contains(s.as_str()))
            .map(|s| json!(s))
            .collect();
        if !copied.is_empty() {
            result.hard.insert("copied_example", copied);
        }
        let count_in = self.checker.count_words(source)?;
        let count_out = self.checker.count_words(output)?;
        let ratio = count_out as f64 / count_in.max(1) as f64;
        let rounded: f64 = format!("{ratio:.2}").parse().expect("a finite count ratio");
        result.word_ratio = Some(rounded);
        if count_in >= 8 && !(0.5..=2.5).contains(&ratio) {
            result.hard.insert("word_ratio", vec![json!(rounded)]);
        }
        let old: BTreeSet<_> = self
            .markup
            .find_iter(source)
            .map(|m| m.map(|m| m.as_str()))
            .collect::<Result<_, _>>()?;
        let new: BTreeSet<_> = self
            .markup
            .find_iter(output)
            .map(|m| m.map(|m| m.as_str()))
            .collect::<Result<_, _>>()?;
        let invented: Vec<_> = new.difference(&old).map(|s| json!(s)).collect();
        if !invented.is_empty() {
            result.hard.insert("invented_markup", invented);
        }
        Ok(result)
    }
}
