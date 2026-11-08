//! **The granite renderer reproduces the R1 held-out prompts token for token.**
//!
//! The R1 adapter was trained and scored on prompts the Hugging Face template
//! rendered and tokenized. Its held-out set records each prompt's token ids in
//! `entries.jsonl`, beside the masked inputs in `input.jsonl` and the folded
//! system prompt in `folded.txt`. This test renders each prompt the way the SPU
//! does, the system turn as the identity prefix and the user turn with the
//! generation opener as the delta, tokenizes both against the artifact's own
//! vocabulary as the SPU does, and compares the ids.
//!
//! An explicit instrument, run with `--ignored` under two variables:
//! `WEAVER_GRANITE_R1_HELDOUT` names the held-out directory and
//! `WEAVER_VOCAB_GRANITE` the granite GGUF. A missing input fails by name.
#![cfg(feature = "gguf")]

use std::path::{Path, PathBuf};

use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use sha2::{Digest as _, Sha256};
use weaver_traits::{ContentBlock, Message, Role};

use weaver_spu::family::{granite, render_generation};

/// The sha256 of the folded system prompt the R1 plan names.
const FOLDED_SHA256: &str = "0fe2ee35a59f6b1e30734af4d7a72e32bd6dbc0e728abc98e32e2c609dad07ed";

/// The held-out set's size, so a truncated deposit fails rather than passes.
const HELD_OUT: usize = 100;

fn required(variable: &str) -> PathBuf {
    let value = std::env::var_os(variable)
        .unwrap_or_else(|| panic!("{variable} is not set, so nothing was compared"));
    PathBuf::from(value)
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()))
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    String::from_utf8(read(path))
        .unwrap_or_else(|_| panic!("{} is not UTF-8", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("{} holds a bad line: {error}", path.display()))
        })
        .collect()
}

fn text(role: Role, text: String) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text { text }],
    }
}

fn ids(model: &LlamaModel, rendered: &str) -> Vec<i64> {
    model
        .str_to_token(rendered, AddBos::Never)
        .expect("the rendering tokenizes")
        .iter()
        .map(|token| i64::from(token.0))
        .collect()
}

/// Perturbation: render the delta with `qwen2::GENERATION_OPENER` and every
/// prompt fails, two ids short at its end.
#[test]
#[ignore = "needs the R1 held-out deposit and the granite GGUF"]
fn the_granite_renderer_reproduces_the_r1_held_out_prompt_ids() {
    let heldout = required("WEAVER_GRANITE_R1_HELDOUT");
    let vocabulary = required("WEAVER_VOCAB_GRANITE");
    assert!(
        vocabulary.is_file(),
        "WEAVER_VOCAB_GRANITE names no regular file at {}",
        vocabulary.display()
    );

    let folded = read(&heldout.join("folded.txt"));
    assert_eq!(
        format!("{:x}", Sha256::digest(&folded)),
        FOLDED_SHA256,
        "folded.txt is not the prompt the R1 plan names"
    );
    let system = String::from_utf8(folded).expect("folded.txt is UTF-8");
    let inputs = lines(&heldout.join("input.jsonl"));
    let entries = lines(&heldout.join("entries.jsonl"));
    assert_eq!(inputs.len(), HELD_OUT, "input.jsonl");
    assert_eq!(entries.len(), HELD_OUT, "entries.jsonl");

    let backend = LlamaBackend::init().expect("the llama backend initialises");
    let params = LlamaModelParams::default()
        .with_vocab_only(true)
        .with_n_gpu_layers(0);
    let model = LlamaModel::load_from_file(&backend, &vocabulary, &params)
        .expect("the granite vocabulary loads");

    let family = granite::renderer();
    let prefix = family
        .render_identity(&[text(Role::System, system)])
        .expect("the system turn renders");
    let prefix_ids = ids(&model, &prefix);

    let mut mismatched: Vec<String> = Vec::new();
    for (input, entry) in inputs.iter().zip(&entries) {
        let id = entry["id"].as_str().expect("an entry carries its id");
        assert_eq!(input["id"], entry["id"], "the two files are in one order");
        let masked = input["text"].as_str().expect("an input carries its text");
        let delta = render_generation(
            family,
            &[text(Role::User, format!("<<<TEXT\n{masked}\nTEXT>>>"))],
            &[],
            granite::GENERATION_OPENER,
        )
        .expect("the user turn renders");
        let mut ours = prefix_ids.clone();
        ours.extend(ids(&model, &delta));
        let expected: Vec<i64> = entry["input_ids"]
            .as_array()
            .expect("an entry carries input_ids")
            .iter()
            .map(|value| value.as_i64().expect("an id is an integer"))
            .collect();
        if ours != expected {
            let at = ours
                .iter()
                .zip(&expected)
                .position(|(left, right)| left != right)
                .unwrap_or(ours.len().min(expected.len()));
            mismatched.push(format!(
                "{id}: first difference at {at} of {} (ours) and {} (recorded)",
                ours.len(),
                expected.len()
            ));
        }
    }
    assert!(
        mismatched.is_empty(),
        "{} of {HELD_OUT} prompts differ:\n  {}",
        mismatched.len(),
        mismatched.join("\n  ")
    );
}
