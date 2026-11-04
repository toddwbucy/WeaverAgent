//! Workflow 6.01 and 4.11. Mechanical checks from ste_check at 9881da6.

use crate::lexicon::{Lexicon, lemmas};
use crate::python_re::{compile, whitespace};
use fancy_regex::Regex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, fancy_regex::Error>;

#[derive(Debug, Clone, Default)]
pub struct CheckResult {
    pub hard: BTreeMap<&'static str, Vec<Value>>,
    pub soft: BTreeMap<&'static str, Vec<Value>>,
    pub sentences: Option<usize>,
    pub sentences_over_20: Option<usize>,
    pub word_ratio: Option<f64>,
}

impl CheckResult {
    pub fn ok(&self) -> bool {
        self.hard.is_empty()
    }

    pub fn to_json(&self) -> Value {
        let mut value = json!({"ok": self.ok(), "hard": self.hard, "soft": self.soft});
        if let Some(n) = self.sentences {
            value["sentences"] = json!(n);
        }
        if let Some(n) = self.sentences_over_20 {
            value["sentences_over_20"] = json!(n);
        }
        if let Some(n) = self.word_ratio {
            value["word_ratio"] = json!(n);
        }
        value
    }
}

pub struct Checker {
    pub(crate) lex: Lexicon,
    instruction_limit: usize,
    description_limit: usize,
    placeholder: Regex,
    word: Regex,
    contraction: Regex,
    placeholder_gap: Regex,
    object_next: Regex,
    markup: Regex,
    heading: Regex,
    sentence_break: Regex,
    instruction: Regex,
}

impl Checker {
    pub fn new(lex: Lexicon, instruction_limit: usize, description_limit: usize) -> Result<Self> {
        Ok(Self {
            lex,
            instruction_limit,
            description_limit,
            placeholder: compile(r"\[\[C\d+\]\]")?,
            word: compile("[A-Za-z][A-Za-z'\u{2019}-]*")?,
            contraction: compile(concat!(
                "(?i)\\b\\w+(?:n['\u{2019}]t|['\u{2019}](?:ll|re|ve|m|d))\\b",
                "|\\b(?:let|it|that|there|here|what|who|where|how|he|she)['\u{2019}]s\\b"
            ))?,
            placeholder_gap: compile(r"\A\s*\x00(?:\s*\x00)*\s*\z")?,
            object_next: compile(concat!(
                r"(?i)^\s*(?:(?:a|an|the|this|these|those|my|our|your|its|",
                r"their|his|her|all|each|every)\b)"
            ))?,
            markup: compile(r"^\s*(#{1,6}\s+|[-*+]\s+|\d+(\.\d+)*[.)]\s+|>\s*)")?,
            heading: compile(r"^\s*#{1,6}\s")?,
            sentence_break: compile(r#"(?<=[.!?:])\s+(?=[A-Z\["(])"#)?,
            instruction: compile(concat!(
                r"^(?:(?:If|When|Before|After|Then|First|Next)\b[^,]*,\s*)?",
                r"([A-Za-z]+)"
            ))?,
        })
    }

    pub fn count_words(&self, sentence: &str) -> Result<usize> {
        let text = self.placeholder.try_replacen(sentence, 0, " PH ")?;
        Ok(text
            .split(whitespace)
            .filter(|t| t.chars().any(|c| c.is_ascii_alphanumeric()))
            .count())
    }

    pub fn sentences(&self, text: &str) -> Result<Vec<String>> {
        let mut blocks = Vec::new();
        let mut current = Vec::new();
        for raw in split_lines(text) {
            let nonempty = !raw.trim_matches(whitespace).is_empty();
            if !nonempty || self.markup.is_match(raw)? {
                if !current.is_empty() {
                    blocks.push(current.join(" "));
                }
                current.clear();
            }
            if nonempty {
                let clean = self
                    .markup
                    .try_replacen(raw, 0, "")?
                    .replace("**", "")
                    .replace("__", " ")
                    .replace('`', "");
                current.push(clean.trim_matches(whitespace).to_owned());
                if self.heading.is_match(raw)? {
                    blocks.push(current.join(" "));
                    current.clear();
                }
            }
        }
        if !current.is_empty() {
            blocks.push(current.join(" "));
        }
        let mut out = Vec::new();
        for block in blocks {
            for sentence in self.sentence_break.split(&block) {
                let sentence = sentence?.trim_matches(whitespace);
                if !sentence.is_empty() {
                    out.push(sentence.to_owned());
                }
            }
        }
        Ok(out)
    }

    pub fn is_instruction(&self, sentence: &str) -> Result<bool> {
        let text = self.placeholder.try_replacen(sentence, 0, "PH")?;
        Ok(self.instruction.captures(text.as_ref())?.is_some_and(|m| {
            self.lex
                .verb_bases
                .contains(&m.get(1).unwrap().as_str().to_lowercase())
        }))
    }

    pub fn check(
        &self,
        output: &str,
        source: &str,
        names: &BTreeSet<String>,
    ) -> Result<CheckResult> {
        let mut result = CheckResult::default();
        let (mut want, mut got) = (
            self.matches(&self.placeholder, source)?,
            self.matches(&self.placeholder, output)?,
        );
        want.sort();
        got.sort();
        if want != got {
            let missing: BTreeSet<_> = want.iter().filter(|p| !got.contains(p)).collect();
            let extra: BTreeSet<_> = got
                .iter()
                .filter(|p| {
                    got.iter().filter(|x| x == p).count() > want.iter().filter(|x| x == p).count()
                })
                .collect();
            result.hard.insert(
                "placeholders",
                vec![json!({"missing": missing,
                "extra_or_repeated": extra})],
            );
        }
        let prose = self.placeholder.try_replacen(output, 0, " ")?;
        if prose.contains('\x3b') {
            result.hard.insert("semicolon", vec![json!("\x3b")]);
        }
        let contractions: BTreeSet<_> = self
            .matches(&self.contraction, &prose)?
            .into_iter()
            .collect();
        insert_words(&mut result.hard, "contractions", contractions);
        self.check_words(output, source, names, &mut result)?;
        let sentences = self.sentences(output)?;
        let mut long = Vec::new();
        let mut over_20 = 0;
        for sentence in &sentences {
            let words = self.count_words(sentence)?;
            over_20 += usize::from(words > 20);
            let limit = if self.is_instruction(sentence)? {
                self.instruction_limit
            } else {
                self.description_limit
            };
            if words > limit {
                long.push(
                    json!({"sentence": sentence.chars().take(200).collect::<String>(),
                "words": words, "limit": limit}),
                );
            }
        }
        if !long.is_empty() {
            result.hard.insert("length", long);
        }
        result.sentences = Some(sentences.len());
        result.sentences_over_20 = Some(over_20);
        Ok(result)
    }

    fn matches(&self, pattern: &Regex, text: &str) -> Result<Vec<String>> {
        pattern
            .find_iter(text)
            .map(|m| m.map(|m| m.as_str().to_owned()))
            .collect()
    }

    fn check_words(
        &self,
        output: &str,
        source: &str,
        names: &BTreeSet<String>,
        result: &mut CheckResult,
    ) -> Result<()> {
        let mut words = self
            .placeholder
            .try_replacen(output, 0, " \0 ")?
            .into_owned();
        for phrase in &self.lex.phrases {
            words = phrase
                .try_replacen(&words, 0, |c: &fancy_regex::Captures<'_, str>| {
                    c.get(1).unwrap().as_str().to_owned()
                })?
                .into_owned();
        }
        let source = self.placeholder.try_replacen(source, 0, " ")?;
        let input_caps: BTreeSet<_> = self
            .matches(&self.word, &source)?
            .into_iter()
            .filter(|s| s.chars().count() >= 2 && is_upper(s))
            .collect();
        let caps_style = input_caps.iter().any(|t| {
            !self.lex.caps_names.contains(t)
                && (self.lex.is_dictionary_word(&t.to_lowercase()) || t.chars().count() >= 5)
        });
        let (mut verbs, mut ing, mut unapproved, mut unknown, mut caps) = (
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let (mut previous, mut previous_end) = (String::new(), 0);
        for matched in self.word.find_iter(&words) {
            let matched = matched?;
            let gap = &words[previous_end..matched.start()];
            let nominal = NOMINAL.contains(&previous.as_str())
                && ((!gap.is_empty() && gap.chars().all(whitespace))
                    || (self.placeholder_gap.is_match(gap)?
                        && !self.object_next.is_match(&words[matched.end()..])?));
            let token = matched.as_str().trim_matches(['\'', '\u{2019}', '-']);
            let low = token.to_lowercase();
            previous = low.clone();
            previous_end = matched.end();
            if low.is_empty() || low == "i" || self.looks_technical(token, names, &input_caps) {
                continue;
            }
            if token.chars().count() >= 2 && is_upper(token) && !caps_style {
                caps.insert(token.to_owned());
            }
            if self.lex.approved.contains_key(&low) || self.lex.technical_verbs.contains(&low) {
                continue;
            }
            let tech_noun = self.lex.technical_nouns.contains(&low);
            let candidates = lemmas(&low);
            let verb = candidates
                .iter()
                .find(|c| self.lex.unapproved_verbs.contains_key(*c));
            let inflected = verb.is_some_and(|v| *v != low);
            let noun_ok = verb.is_some_and(|v| {
                self.lex.approved.contains_key(v) || self.lex.technical_nouns.contains(v)
            });
            if let Some(verb) = verb
                && !tech_noun
                && nominal
                && !low.ends_with("ing")
            {
                let parts = self
                    .lex
                    .unapproved
                    .get(&low)
                    .into_iter()
                    .flatten()
                    .chain(self.lex.unapproved.get(verb).into_iter().flatten());
                if parts.into_iter().any(|p| p == "n" || p == "adj") {
                    unapproved.insert(low);
                }
                continue;
            }
            if verb.is_some()
                && !tech_noun
                && ((inflected && (low.ends_with("ed") || low.ends_with("ing"))) || !noun_ok)
            {
                verbs.insert(low);
                continue;
            }
            if low.ends_with("ing")
                && low.chars().count() > 4
                && !tech_noun
                && !NON_VERB_ING.contains(&low.as_str())
            {
                ing.insert(low);
                continue;
            }
            if tech_noun || noun_ok {
                continue;
            }
            if candidates
                .iter()
                .any(|c| self.lex.unapproved.contains_key(c))
            {
                unapproved.insert(low);
            } else if !candidates.iter().any(|c| self.lex.approved.contains_key(c)) {
                unknown.insert(low);
            }
        }
        insert_words(&mut result.hard, "capitals", caps);
        insert_words(&mut result.hard, "verbs", verbs);
        insert_words(&mut result.hard, "ing", ing);
        insert_words(&mut result.soft, "unapproved_words", unapproved);
        insert_words(&mut result.soft, "unknown_words", unknown);
        Ok(())
    }

    fn looks_technical(
        &self,
        token: &str,
        names: &BTreeSet<String>,
        input_caps: &BTreeSet<String>,
    ) -> bool {
        if names.contains(token) || names.contains(&token.to_lowercase()) {
            return true;
        }
        if token
            .chars()
            .any(|c| c == '_' || c == '.' || c.is_ascii_digit())
            || token
                .as_bytes()
                .windows(2)
                .any(|p| p[0].is_ascii_lowercase() && p[1].is_ascii_uppercase())
        {
            return true;
        }
        if token.chars().count() < 2 || !is_upper(token) {
            return false;
        }
        self.lex.caps_names.contains(token)
            || input_caps.contains(token)
            || !self.lex.is_dictionary_word(&token.to_lowercase())
    }
}

fn insert_words(
    map: &mut BTreeMap<&'static str, Vec<Value>>,
    key: &'static str,
    words: BTreeSet<String>,
) {
    if !words.is_empty() {
        map.insert(key, words.into_iter().map(Value::String).collect());
    }
}

fn is_upper(text: &str) -> bool {
    text.chars().any(char::is_uppercase) && !text.chars().any(char::is_lowercase)
}

/// Python splitlines treats CRLF as one break and includes Unicode separators.
pub(crate) fn split_lines(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            result.push(&text[start..index]);
            start = index + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                start = chars.next().unwrap().0 + 1;
            }
        }
    }
    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

const NOMINAL: &[&str] = &[
    "a", "an", "the", "my", "our", "your", "its", "their", "his", "her", "no", "in", "on", "of",
    "for", "with", "without", "by", "from", "at", "into", "onto", "about", "under", "over",
    "between", "per",
];
const NON_VERB_ING: &[&str] = &[
    "string",
    "thing",
    "nothing",
    "something",
    "anything",
    "everything",
    "during",
    "spring",
    "ring",
    "king",
    "sibling",
    "ceiling",
    "morning",
    "evening",
    "bring",
];
