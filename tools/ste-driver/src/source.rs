//! Workflow 4.11 and 6.01. Read the pinned lexicon once before conversion.

use crate::lexicon::{Lexicon, LexiconInput};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const APPROVED_WORDS_SHA256: &str =
    "c622bc230bc0c29512ef16990f527bf32a5ffaffbe955f73751251a7e5ab4693";

const TECHNICAL_VERBS_SHA256: &str =
    "004c54260fbdaf99891c379140abeaecf48063dd536ce06cec276fcd27d3cd81";
const TECHNICAL_NOUNS_SHA256: &str =
    "8c1cee4d6b7d1cd0b02f681b68111c1155c62c1d2473184642d87fb75ff217bd";
const TECHNICAL_NAMES_SHA256: &str =
    "84c4d8bea0e4b376ee5fd5c9c736f318fd58d0e412e33e4040f7bc1dcb4b5546";
const FIXED_MAPPINGS_SHA256: &str =
    "6d0533ab3aa45965df9ba44a59baada4c98c2bc74472d55da3745831d2710ddf";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

impl Lexicon {
    /// Hash the bytes that are parsed. No lexicon file is changed or generated.
    pub fn load_pinned(train_root: &Path, approved_words: &Path) -> Result<Self> {
        let words: Value =
            serde_json::from_slice(&read_pinned(approved_words, APPROVED_WORDS_SHA256)?)?;
        let verbs = read_toml(
            &train_root.join("ste/technical-verbs.toml"),
            TECHNICAL_VERBS_SHA256,
        )?;
        let nouns = read_toml(
            &train_root.join("ste/technical-nouns.toml"),
            TECHNICAL_NOUNS_SHA256,
        )?;
        let names = read_toml(
            &train_root.join("ste/technical-names.toml"),
            TECHNICAL_NAMES_SHA256,
        )?;
        let fixed = read_toml(
            &train_root.join("ste/fixed-mappings.toml"),
            FIXED_MAPPINGS_SHA256,
        )?;
        let technical_verbs = verbs["verbs"]
            .as_object()
            .ok_or("technical verbs must be a table")?
            .keys()
            .cloned()
            .collect();
        let mut technical_nouns: BTreeSet<String> = serde_json::from_value(nouns["nouns"].clone())?;
        technical_nouns.extend(serde_json::from_value::<Vec<String>>(
            words["technical_noun_examples"].clone(),
        )?);
        let mut caps_names = BTreeSet::new();
        for list in names
            .as_object()
            .ok_or("technical names must be a table")?
            .values()
        {
            caps_names.extend(serde_json::from_value::<Vec<String>>(list.clone())?);
        }
        let mut unapproved_verbs = BTreeMap::new();
        for verb in words["unapproved_verbs"]
            .as_array()
            .ok_or("unapproved verbs must be a list")?
        {
            unapproved_verbs.insert(
                verb["word"]
                    .as_str()
                    .ok_or("verb word must be text")?
                    .to_owned(),
                serde_json::from_value(verb["alternatives"].clone())?,
            );
        }
        let mut approved_verbs = Vec::new();
        for verb in words["verbs"]
            .as_array()
            .ok_or("approved verbs must be a list")?
        {
            approved_verbs.push(
                verb["key"]
                    .as_str()
                    .ok_or("verb key must be text")?
                    .to_owned(),
            );
        }
        Self::build(LexiconInput {
            approved: serde_json::from_value(words["approved_words"].clone())?,
            unapproved: serde_json::from_value(words["unapproved_words"].clone())?,
            unapproved_verbs,
            technical_verbs,
            technical_nouns: technical_nouns.into_iter().collect(),
            approved_verbs,
            caps_names,
            fixed: serde_json::from_value(fixed["mappings"].clone())?,
        })
    }
}

fn read_pinned(path: &Path, expected: &str) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected {
        return Err(format!("source hash mismatch: {}", path.display()).into());
    }
    Ok(bytes)
}

fn read_toml(path: &Path, expected: &str) -> Result<Value> {
    let bytes = read_pinned(path, expected)?;
    let value: toml::Value = toml::from_str(std::str::from_utf8(&bytes)?)?;
    Ok(serde_json::to_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::read_pinned;

    #[test]
    fn pinned_reads_keep_exact_bytes_and_refuse_changed_source() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("ste-pinned-read-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("source");
        std::fs::write(&path, b"one\r\n").unwrap();
        let expected = "5259d46a49644bf76792231ef7315b5293677c49ddd7e69d95557013e10320d4";
        assert_eq!(read_pinned(&path, expected).unwrap(), b"one\r\n");
        std::fs::write(&path, b"one\n").unwrap();
        let error = read_pinned(&path, expected).unwrap_err();
        assert!(error.to_string().contains("source hash mismatch"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
