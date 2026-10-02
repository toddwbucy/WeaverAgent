//! The model-free operations python-spu-Spec section 3.1 names for `knobs`, `stops`,
//! `registry` and `partition`: each runs the weaver-spu item itself on the inputs the
//! suite supplies and answers its result or its refusal, spelled as the Rust names it.
use serde_json::{Value, json};
use weaver_spu::sampling::{Disposition, KnobRefusal, Knobs, SessionParameters, TunableValues};

fn disposition<T: Copy>(
    v: &Value,
    name: &str,
    take: impl Fn(&Value) -> Option<T>,
) -> Result<Disposition<T>, String> {
    match &v[name] {
        Value::String(s) if s == "tunable" => Ok(Disposition::OperatorTunable),
        frozen => Ok(Disposition::Frozen(
            take(&frozen["frozen"]).ok_or(format!("frozen {name}"))?,
        )),
    }
}

fn refusal(refused: KnobRefusal) -> Value {
    match refused {
        KnobRefusal::Unsupplied { knob } => json!({"unsupplied": knob}),
        KnobRefusal::NotACount { knob, supplied } => {
            json!({"not_a_count": {"knob": knob, "supplied": supplied}})
        }
    }
}

/// sampling.rs `Knobs::resolve` then `SessionParameters::resolve`, as the binary's
/// `resolve_effective` runs them, over dispositions and tunable values the suite supplies.
pub fn knobs(v: &Value) -> Result<Value, String> {
    let d = &v["dispositions"];
    let f32_of = |x: &Value| x.as_f64().map(|f| f as f32);
    let u32_of = |x: &Value| x.as_u64().and_then(|n| u32::try_from(n).ok());
    let knobs = Knobs {
        temperature: disposition(d, "temperature", f32_of)?,
        top_k: disposition(d, "top-k", u32_of)?,
        top_p: disposition(d, "top-p", f32_of)?,
        repetition_penalty: disposition(d, "repetition-penalty", f32_of)?,
        repetition_window: disposition(d, "repetition-window", u32_of)?,
        seed: disposition(d, "seed", |x| x.as_u64())?,
    };
    let session = SessionParameters {
        context_capacity: disposition(d, "context-capacity", u32_of)?,
        max_tokens_per_turn: disposition(d, "max-tokens-per-turn", |x| {
            x.as_u64().map(|n| n as usize)
        })?,
    };
    let supplied: TunableValues =
        serde_json::from_value(v["supplied"].clone()).map_err(|e| e.to_string())?;
    let effective = match knobs.resolve(&supplied) {
        Ok(k) => k,
        Err(e) => return Ok(json!({"refused": refusal(e)})),
    };
    let parameters = match session.resolve(&supplied) {
        Ok(s) => s,
        Err(e) => return Ok(json!({"refused": refusal(e)})),
    };
    let tunable = [knobs.tunable_names(), session.tunable_names()].concat();
    Ok(json!({"effective": {
        "temperature": effective.temperature, "top-k": effective.top_k, "top-p": effective.top_p,
        "repetition-penalty": effective.repetition_penalty,
        "repetition-window": effective.repetition_window, "seed": effective.seed,
        "context-capacity": parameters.context_capacity,
        "max-tokens-per-turn": parameters.max_tokens_per_turn},
        "tunable": tunable}))
}

/// residency.rs `promote_stop_conditions` over declared conditions, an end of sequence
/// and a vocabulary the suite supplies as each condition's token ids.
pub fn stops(v: &Value) -> Result<Value, String> {
    use weaver_spu::decoder::backend::TokenId;
    let declared: Vec<&'static str> = v["declared"]
        .as_array()
        .ok_or("declared")?
        .iter()
        .map(|c| {
            c.as_str()
                .map(|s| &*Box::leak(s.to_string().into_boxed_str()))
        })
        .collect::<Option<_>>()
        .ok_or("declared strings")?;
    let eos = TokenId(v["eos"].as_u64().ok_or("eos")? as u32);
    let vocabulary = v["vocabulary"].clone();
    let tokenize =
        |condition: &str| -> Result<Vec<TokenId>, weaver_spu::decoder::backend::DecodeFault> {
            Ok(vocabulary[condition]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(|i| i.as_u64())
                        .map(|i| TokenId(i as u32))
                        .collect()
                })
                .unwrap_or_default())
        };
    match weaver_spu::residency::promote_stop_conditions(&declared, eos, tokenize) {
        Ok(set) => Ok(
            json!({"tokens": set.tokens.iter().map(|t| t.0).collect::<Vec<_>>(),
            "terminator": set.terminator.0, "unpromoted": set.unpromoted}),
        ),
        Err(fault) => Ok(json!({"fault": format!("{fault:?}")})),
    }
}

/// family/mod.rs `select` over a family name and template, `judge_width` over a device
/// count, readout.rs `judge` over an election, and the key's `normalised_key`.
pub fn registry(v: &Value) -> Result<Value, String> {
    use weaver_spu::family::{FamilyName, judge_width, normalised_key, select};
    let name = v["family"].as_str().ok_or("family")?;
    let template = v["template"].as_str();
    let key = normalised_key(name);
    let declaration = match select(&FamilyName(name.into()), template) {
        Ok(declaration) => declaration,
        Err(refused) => return Ok(json!({"key": key, "refused": format!("{refused:?}")})),
    };
    let width = judge_width(declaration, v["width"].as_u64().ok_or("width")? as u32)
        .map(|()| Value::Null)
        .unwrap_or_else(|e| json!(format!("{e:?}")));
    let readout = weaver_spu::readout::judge(
        weaver_spu::readout::ReadoutElection(v["readout"].as_bool().unwrap_or(false)),
        declaration,
    )
    .map(|()| Value::Null)
    .unwrap_or_else(|e| json!(format!("{e:?}")));
    Ok(
        json!({"key": key, "selected": declaration.family, "widths": declaration.shard_widths,
        "width_refused": width, "readout_refused": readout}),
    )
}

/// measurement.rs `PromptPartition::new` over offsets and a text length.
pub fn partition(v: &Value) -> Result<Value, String> {
    use weaver_spu::measurement::{PartitionDefect, PromptPartition};
    let offsets: Vec<usize> =
        serde_json::from_value(v["offsets"].clone()).map_err(|e| e.to_string())?;
    let text_len = v["text_len"].as_u64().ok_or("text_len")? as usize;
    Ok(match PromptPartition::new(offsets, text_len) {
        Ok(p) => json!({"offsets": p.offsets(), "text_len": p.text_len(), "blocks": p.blocks()}),
        Err(PartitionDefect::Empty) => json!({"defect": {"kind": "empty"}}),
        Err(PartitionDefect::NotAscending { at }) => {
            json!({"defect": {"kind": "not_ascending", "at": at}})
        }
        Err(PartitionDefect::ShortOfEnd { last, text_len }) => {
            json!({"defect": {"kind": "short_of_end", "last": last, "text_len": text_len}})
        }
    })
}
