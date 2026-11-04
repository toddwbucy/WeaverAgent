//! Workflow 4.11. Match the reference's Unicode 16 character classes.

use fancy_regex::Regex;

mod classes {
    include!("unicode_classes.rs");
}

/// Compile the fixed source patterns with Python's word and space rules.
/// The patterns use no escaped backslash before a class escape.
pub(crate) fn compile(pattern: &str) -> Result<Regex, fancy_regex::Error> {
    let word = format!("[{}]", classes::WORD);
    let boundary = format!("(?:(?<={word})(?!{word})|(?<!{word})(?={word}))");
    let pattern = pattern
        .replace(r"\b", &boundary)
        .replace(r"\w", &word)
        .replace(r"\d", &format!("[{}]", classes::DECIMAL))
        .replace(r"\s", &format!("[{}]", classes::SPACE))
        .replace(r"\S", &format!("[^{}]", classes::SPACE));
    Regex::new(&pattern)
}

pub(crate) fn whitespace(ch: char) -> bool {
    matches!(ch,
        '\t'..='\r' | '\u{1c}'..=' ' | '\u{85}' | '\u{a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}')
}
