//! Workflow 6.01 and 4.11. Lexicon rules from ste_check at 9881da6.

use crate::python_re::{compile, whitespace};
use fancy_regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexiconInput {
    pub approved: BTreeMap<String, Vec<String>>,
    pub unapproved: BTreeMap<String, Vec<String>>,
    pub unapproved_verbs: BTreeMap<String, Vec<String>>,
    pub technical_verbs: Vec<String>,
    pub technical_nouns: Vec<String>,
    pub approved_verbs: Vec<String>,
    pub caps_names: BTreeSet<String>,
    pub fixed: BTreeMap<String, String>,
}

pub struct Lexicon {
    pub(crate) approved: BTreeMap<String, Vec<String>>,
    pub(crate) unapproved: BTreeMap<String, Vec<String>>,
    pub(crate) unapproved_verbs: BTreeMap<String, Vec<String>>,
    pub(crate) technical_verbs: BTreeSet<String>,
    pub(crate) technical_nouns: BTreeSet<String>,
    pub(crate) verb_bases: BTreeSet<String>,
    pub(crate) phrases: Vec<Regex>,
    pub(crate) caps_names: BTreeSet<String>,
    pub(crate) fixed: BTreeMap<String, String>,
}

impl Lexicon {
    /// Match Lexicon.build. Source owners must check the source hashes first.
    pub fn build(input: LexiconInput) -> Result<Self, Box<dyn std::error::Error>> {
        let mut technical_verbs = BTreeSet::new();
        let mut phrases = Vec::new();
        let mut verb_bases = BTreeSet::new();
        for verb in &input.technical_verbs {
            let mut parts = verb.split(whitespace).filter(|s| !s.is_empty());
            let head = parts.next().ok_or("empty technical verb")?;
            let tail: Vec<_> = parts.collect();
            verb_bases.insert(head.to_owned());
            let forms = verb_forms(head);
            technical_verbs.extend(forms.iter().cloned());
            if !tail.is_empty() {
                let mut forms: Vec<_> = forms.into_iter().collect();
                forms.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
                let tail = tail
                    .iter()
                    .map(|p| regex::escape(p))
                    .collect::<Vec<_>>()
                    .join(r"\s+");
                phrases.push(compile(&format!(
                    r"(?i)\b({})\s+{tail}\b",
                    forms.join("|")
                ))?);
            }
        }
        for verb in &input.approved_verbs {
            let head = verb
                .split(whitespace)
                .find(|s| !s.is_empty())
                .ok_or("empty approved verb")?;
            verb_bases.insert(head.to_owned());
        }
        let mut technical_nouns = BTreeSet::new();
        for noun in &input.technical_nouns {
            for part in noun
                .to_lowercase()
                .split(whitespace)
                .filter(|s| !s.is_empty())
            {
                technical_nouns.extend([part.to_owned(), format!("{part}s"), format!("{part}es")]);
            }
        }
        Ok(Self {
            approved: input.approved,
            unapproved: input.unapproved,
            unapproved_verbs: input.unapproved_verbs,
            technical_verbs,
            technical_nouns,
            verb_bases,
            phrases,
            caps_names: input.caps_names,
            fixed: input.fixed,
        })
    }

    pub(crate) fn is_dictionary_word(&self, word: &str) -> bool {
        self.approved.contains_key(word)
            || self.unapproved.contains_key(word)
            || self.technical_verbs.contains(word)
            || lemmas(word)
                .iter()
                .skip(1)
                .any(|c| self.unapproved_verbs.contains_key(c) || self.approved.contains_key(c))
    }
}

/// The source deliberately returns candidates beyond dictionary membership.
pub fn lemmas(token: &str) -> Vec<String> {
    let mut out = vec![token.to_owned()];
    for (suffix, addition) in [
        ("ies", "y"),
        ("ied", "y"),
        ("es", ""),
        ("s", ""),
        ("ed", ""),
        ("ed", "e"),
        ("d", ""),
        ("ing", ""),
        ("ing", "e"),
    ] {
        if let Some(base) = token.strip_suffix(suffix)
            && base.chars().count() >= 2
        {
            let stem = format!("{base}{addition}");
            out.push(stem.clone());
            let mut chars = stem.chars().rev();
            let last = chars.next().unwrap();
            if matches!(suffix, "ed" | "ing")
                && stem.chars().count() > 2
                && Some(last) == chars.next()
            {
                out.push(stem[..stem.len() - last.len_utf8()].to_owned());
            }
        }
    }
    out
}

pub fn verb_forms(base: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<_> = ["", "s", "es", "ed", "d", "ing"]
        .into_iter()
        .map(|suffix| format!("{base}{suffix}"))
        .collect();
    if let Some(stem) = base.strip_suffix('e') {
        out.insert(format!("{stem}ing"));
    }
    if let Some(stem) = base.strip_suffix('y') {
        out.extend([format!("{stem}ies"), format!("{stem}ied")]);
    }
    let tail: Vec<_> = base.chars().rev().take(3).collect();
    if let [last, vowel, prior] = tail.as_slice()
        && "bdgklmnprt".contains(*last)
        && "aeiou".contains(*vowel)
        && !"aeiou".contains(*prior)
    {
        out.extend([format!("{base}{last}ed"), format!("{base}{last}ing")]);
    }
    out
}
