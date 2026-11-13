//! Workflow 6.08. ste_check.repair_note and ste_kit_run.one at 9881da6.

use crate::check::{CheckResult, Checker};
use crate::lexicon::lemmas;
use crate::python_re::whitespace;
use serde_json::Value;

impl Checker {
    /// Render the pinned checker's notes from a result produced by this checker.
    pub fn repair_note(&self, result: &CheckResult) -> String {
        let mut lines = Vec::new();
        let hard = &result.hard;
        if let Some(placeholders) = hard.get("placeholders") {
            let p = &placeholders[0];
            let missing = p["missing"].as_array().unwrap();
            if !missing.is_empty() {
                lines.push(format!(
                    "These placeholders are missing. Put each one back exactly once: {}",
                    joined(missing)
                ));
            }
            let extra = p["extra_or_repeated"].as_array().unwrap();
            if !extra.is_empty() {
                lines.push(format!(
                    concat!(
                        "These placeholders occur too many times or are not in the text. ",
                        "Use each one only once: {}"
                    ),
                    joined(extra)
                ));
            }
        }
        for word in strings(hard.get("verbs")) {
            let lemma = lemmas(word)
                .into_iter()
                .find(|c| self.lex.unapproved_verbs.contains_key(c))
                .unwrap_or_else(|| word.to_owned());
            if let Some(replacement) = self.lex.fixed.get(&lemma) {
                lines.push(format!(
                    concat!(
                        "\"{}\" is not an approved verb. ",
                        "Write \"{}\" for it (a fixed rule)."
                    ),
                    word, replacement
                ));
                continue;
            }
            let alts = self
                .lex
                .unapproved_verbs
                .get(&lemma)
                .map(|x| x.join(", "))
                .unwrap_or_default();
            let alts = if alts.is_empty() {
                "none\x3b write the sentence in a different way"
            } else {
                &alts
            };
            lines.push(format!(
                "\"{word}\" is not an approved verb. Alternatives the standard gives: {alts}."
            ));
        }
        for word in strings(hard.get("ing")) {
            lines.push(format!(
                "\"{word}\" is an -ing form. Write the sentence in a different way."
            ));
        }
        for word in strings(hard.get("capitals")) {
            lines.push(format!(
                concat!(
                    "\"{}\" is a normal word written in capitals. Write it in lowercase, ",
                    "or write the sentence in a different way."
                ),
                word
            ));
        }
        if let Some(contractions) = hard.get("contractions") {
            lines.push(format!("Do not use contractions: {}", joined(contractions)));
        }
        if hard.contains_key("semicolon") {
            lines.push("Do not use the semicolon (\x3b).".to_owned());
        }
        if let Some(lengths) = hard.get("length") {
            for value in lengths {
                lines.push(format!(
                    "This sentence has {} words\x3b the maximum is {}. Divide it: \"{}\"",
                    value["words"],
                    value["limit"],
                    value["sentence"].as_str().unwrap()
                ));
            }
        }
        if let Some(markup) = hard.get("invented_markup")
            && !markup.is_empty()
        {
            lines.push(format!(
                "Your rewrite has markup that is not in the text: {}. Remove it.",
                joined(markup)
            ));
        }
        if let Some(literals) = hard.get("altered_literals")
            && !literals.is_empty()
        {
            lines.push(format!(
                concat!(
                    "Your rewrite changed text that is joined to code or a name. Keep this text ",
                    "exactly as it is in the input: {}."
                ),
                joined(literals)
            ));
        }
        lines
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The driver appends these guard notes before the clause 6.08 wrapper.
    pub fn repair_notes_with_guards(&self, result: &CheckResult) -> String {
        let mut notes = self.repair_note(result);
        if result.hard.contains_key("copied_example") {
            notes.push_str(concat!(
                "\n- You copied text from an example. Rewrite only the text ",
                "between <<<TEXT and TEXT>>>."
            ));
        }
        if result.hard.contains_key("word_ratio") {
            notes.push_str(concat!(
                "\n- Your rewrite is much longer or much shorter than the text. ",
                "Keep all of the information and add nothing."
            ));
        }
        let notes = notes.trim_matches(whitespace);
        if notes.is_empty() {
            "- Obey the rules.".to_owned()
        } else {
            notes.to_owned()
        }
    }
}

fn strings(values: Option<&Vec<Value>>) -> impl Iterator<Item = &str> {
    values
        .into_iter()
        .flatten()
        .map(|v| v.as_str().expect("checker word list"))
}

fn joined(values: &[Value]) -> String {
    values
        .iter()
        .map(|v| v.as_str().expect("checker word list"))
        .collect::<Vec<_>>()
        .join(", ")
}
