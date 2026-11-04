//! Workflow 4.04 and 4.11, ste_convert.mask at WeaverTrain 9881da6.
//! The codes retain the exact spans. Masking does not change the dataset.

use crate::python_re::{compile, whitespace};
use fancy_regex::Regex;

type Result<T> = std::result::Result<T, fancy_regex::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskedText {
    pub text: String,
    pub codes: Vec<String>,
}

/// Compile the pinned source patterns once for a driver's lifetime.
pub struct Masker {
    spans: Vec<Regex>,
    quote: Regex,
    fence: Regex,
    not_paragraph: Regex,
    interrupts: Regex,
    listing: Regex,
    aligned: Regex,
    opener: Regex,
    placeholder: Regex,
    aside: Regex,
    identifier: Regex,
    numeric: Regex,
    prefix_word: Regex,
}

impl Masker {
    pub fn new() -> Result<Self> {
        let spans = [
            r"(?s)```.*?```",
            r"`[^`\n]+`",
            r"https?://\S+",
            r"\b[A-Z]\w*(?:Error|Exception|Warning)(?:: [^\n]+|:(?=\n|$))",
            r"</[A-Za-z][\w-]*\s*>|(?<![\w<])<[A-Za-z][\w-]*(?:\s+[^<>\n]*)?/?>",
            r#""\s*""#,
            r#""(?!\s)(?:[^"\\\n]|\\"(?![\s;,.:)\]}]|$)|\\[^"\n]|\\(?=")){1,300}(?<!\s)""#,
            r"(?<![\w/])(?:\.{0,2}/)?(?:[\w.-]+/)+[\w.-]*",
            r"\b[\w-]+\.(?:py|pyx|pyi|txt|md|rst|toml|json|ya?ml|cfg|ini|sh|c|h|cpp|rs|js|ts|csv)\b",
        ]
        .into_iter()
        .map(compile)
        .collect::<Result<_>>()?;
        Ok(Self {
            spans,
            quote: compile(r"(?s)^(?P<lead>[ \t]*(?:(?:[-*+]|\d+[.)])[ \t]+)?)>(?P<rest>.*)")?,
            fence: compile(r"^[ \t]*(?:```|~~~)")?,
            not_paragraph: compile(
                r"^[ \t]*(?:\r?$|#{1,6}(?:[ \t]|\r?$)|```|~~~|(?:[-*_][ \t]*){3,}\r?$|\[\[C\d+\]\][ \t]*\r?$)",
            )?,
            interrupts: compile(
                r"^[ \t]*(?:\r?$|#{1,6}(?:[ \t]|\r?$)|```|~~~|[-*+][ \t]|\d+[.)][ \t]|(?:[-*_][ \t]*){3,}\r?$)",
            )?,
            listing: compile(r"\A[\w.\-\[\]]+ *:(?: .*)?\z")?,
            aligned: compile(r"^\S+ {2,}:")?,
            opener: compile(r"[\w.]*[(\[{]")?,
            placeholder: compile(r"\[\[C\d+\]\]")?,
            aside: compile("\\A\\(\\w+(['\u{2019}]\\w+)?( \\w+(['\u{2019}]\\w+)?){0,3}\\)\\z")?,
            identifier: compile(r"\b(?:[A-Za-z_]\w*\.)+[A-Za-z_]\w*\b|\b\w*_\w*\b")?,
            numeric: compile(r"\A\d+(\.\d+)*\z")?,
            prefix_word: compile(r"\w*\z")?,
        })
    }

    pub fn mask(&self, source: &str) -> Result<MaskedText> {
        let mut codes = Vec::new();
        let mut text = self.listings(source, &mut codes)?;
        for (index, pattern) in self.spans.iter().enumerate() {
            text = replace(pattern, &text, |span| keep(span, &mut codes))?;
            if index == 0 {
                text = self.quotations(&text, &mut codes)?;
            }
        }
        text = self.brackets(&text, &mut codes)?;
        let mut out = String::new();
        let mut position = 0;
        for matched in self.identifier.find_iter(&text) {
            let matched = matched?;
            out.push_str(&text[position..matched.start()]);
            if self.numeric.is_match(matched.as_str())? {
                out.push_str(matched.as_str());
            } else {
                out.push_str(&keep(matched.as_str(), &mut codes));
            }
            position = matched.end();
        }
        out.push_str(&text[position..]);
        Ok(MaskedText { text: out, codes })
    }

    fn listings(&self, text: &str, codes: &mut Vec<String>) -> Result<String> {
        let lines: Vec<_> = text.split('\n').collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            if !self.listing.is_match(lines[i])? {
                out.push(lines[i].to_owned());
                i += 1;
                continue;
            }
            let mut j = i;
            while j < lines.len()
                && (self.listing.is_match(lines[j])?
                    || lines[j].trim_matches(whitespace).is_empty())
            {
                j += 1;
            }
            while j > i && lines[j - 1].trim_matches(whitespace).is_empty() {
                j -= 1;
            }
            let mut aligned = 0;
            for line in &lines[i..j] {
                aligned += usize::from(self.aligned.is_match(line)?);
            }
            if aligned >= 3 {
                codes.push(lines[i..j].join("\n"));
                out.push(format!("[[C{}]]", codes.len()));
            } else {
                out.extend(lines[i..j].iter().map(|line| (*line).to_owned()));
            }
            i = j;
        }
        Ok(out.join("\n"))
    }

    fn quote_step(&self, rest: &str, fence: &mut bool, paragraph: &mut bool) -> Result<()> {
        if self.fence.is_match(rest)? {
            *fence = !*fence;
            *paragraph = false;
        } else if !*fence {
            *paragraph = !self.not_paragraph.is_match(rest)?;
        }
        Ok(())
    }

    fn quotations(&self, text: &str, codes: &mut Vec<String>) -> Result<String> {
        let lines: Vec<_> = text.split('\n').collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let Some(first) = self.quote.captures(lines[i])? else {
                out.push(lines[i].to_owned());
                i += 1;
                continue;
            };
            let lead = first.name("lead").unwrap().as_str();
            let mut block = vec![&lines[i][lead.len()..]];
            let (mut fence, mut paragraph) = (false, false);
            self.quote_step(
                first.name("rest").unwrap().as_str(),
                &mut fence,
                &mut paragraph,
            )?;
            let mut j = i + 1;
            while j < lines.len() {
                let quote = self.quote.captures(lines[j])?;
                if let Some(ref quote) = quote
                    && quote
                        .name("lead")
                        .unwrap()
                        .as_str()
                        .trim_matches(whitespace)
                        .is_empty()
                {
                    self.quote_step(
                        quote.name("rest").unwrap().as_str(),
                        &mut fence,
                        &mut paragraph,
                    )?;
                } else if quote.is_some()
                    || !paragraph
                    || fence
                    || self.interrupts.is_match(lines[j])?
                {
                    break;
                }
                block.push(lines[j]);
                j += 1;
            }
            out.push(format!("{lead}{}", keep(&block.join("\n"), codes)));
            i = j;
        }
        Ok(out.join("\n"))
    }

    fn opens_string(&self, text: &str, end: usize) -> Result<bool> {
        let word = self.prefix_word.find(&text[..end])?.unwrap().as_str();
        Ok(matches!(
            word.to_lowercase().as_str(),
            "" | "r" | "b" | "f" | "u" | "rb" | "br" | "fr" | "rf" | "e"
        ))
    }

    fn brackets(&self, text: &str, codes: &mut Vec<String>) -> Result<String> {
        let mut out = String::new();
        let mut i = 0;
        while let Some(matched) = self.opener.find_from_pos(text, i)? {
            let start = matched.end() - 1;
            if let Some(ph) = self.placeholder.find_from_pos(text, start)?
                && ph.start() == start
                && matched.start() == start
            {
                out.push_str(&text[i..ph.end()]);
                i = ph.end();
                continue;
            }
            let (mut j, mut stack, mut quote) = (start, Vec::new(), None);
            while j < text.len() && text.as_bytes()[j] != b'\n' {
                let ch = text[j..].chars().next().unwrap();
                if let Some(delimiter) = quote {
                    if ch == '\\' {
                        j += ch.len_utf8();
                        if let Some(next) = text[j..].chars().next() {
                            j += next.len_utf8();
                        }
                        continue;
                    }
                    if ch == delimiter {
                        quote = None;
                    }
                } else if matches!(ch, '\'' | '"')
                    && !stack.is_empty()
                    && self.opens_string(text, j)?
                {
                    quote = Some(ch);
                } else if let Some(close) = match ch {
                    '(' => Some(')'),
                    '[' => Some(']'),
                    '{' => Some('}'),
                    _ => None,
                } {
                    stack.push(close);
                } else if stack.last() == Some(&ch) {
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                }
                j += ch.len_utf8();
            }
            if !stack.is_empty()
                || j >= text.len()
                || text.as_bytes()[j] == b'\n'
                || self.aside.is_match(&text[matched.start()..=j])?
            {
                out.push_str(&text[i..matched.end()]);
                i = matched.end();
                continue;
            }
            codes.push(text[matched.start()..=j].to_owned());
            out.push_str(&text[i..matched.start()]);
            out.push_str(&format!("[[C{}]]", codes.len()));
            i = j + 1;
        }
        out.push_str(&text[i..]);
        Ok(out)
    }
}

fn keep(span: &str, codes: &mut Vec<String>) -> String {
    if span.starts_with("[[C") {
        return span.to_owned();
    }
    codes.push(span.to_owned());
    format!("[[C{}]]", codes.len())
}

fn replace(
    pattern: &Regex,
    text: &str,
    mut replacement: impl FnMut(&str) -> String,
) -> Result<String> {
    let mut out = String::new();
    let mut position = 0;
    for matched in pattern.find_iter(text) {
        let matched = matched?;
        out.push_str(&text[position..matched.start()]);
        out.push_str(&replacement(matched.as_str()));
        position = matched.end();
    }
    out.push_str(&text[position..]);
    Ok(out)
}

/// Workflow 6.06. Call only after placeholder and literal checks pass.
pub fn unmask(text: &str, codes: &[String]) -> String {
    let mut out = text.to_owned();
    for n in (1..=codes.len()).rev() {
        out = out.replace(&format!("[[C{n}]]"), &codes[n - 1]);
    }
    out
}
