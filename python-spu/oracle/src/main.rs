mod sampler;
mod session;
use std::io::{self, BufRead};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round<T: DeserializeOwned + Serialize>(v: &str) -> Result<Value, String> {
    let t: T = serde_json::from_str(v).map_err(|e| e.to_string())?;
    serde_json::to_value(t).map_err(|e| e.to_string())
}
fn run(v: Value, raw_body: &str) -> Result<Value, String> {
    let body = v["body"].clone();
    match v["op"].as_str().unwrap_or("") {
        "session" => session::run(&body),
        "rng" => sampler::rng(&v),
        "select" => sampler::select(&v),
        "generation" => sampler::generation(&v),
        "probs" => sampler::probs(&v),
        "weighted" => sampler::weighted(&v),
        "seed" => Ok(json!(weaver_spu::sampling::derived_seed(
            v["seed"].as_u64().ok_or("seed")?,
            &weaver_types::TurnKey(v["turn"].as_str().ok_or("turn")?.into()),
            v["generation"].as_u64().ok_or("generation")?))),
        "measure" => {
            let logits: Vec<f32> = serde_json::from_value(body).map_err(|e| e.to_string())?;
            let token = v["token"].as_u64().ok_or("token")? as usize;
            Ok(json!({"entropy": weaver_spu::measurement::entropy_bits(&logits),
                "surprisal": weaver_spu::measurement::surprisal_bits(&logits,token),
                "field": weaver_spu::measurement::field(&logits,token,3)}))
        },
        // weaver-spu artifact.rs's resolve then pin, over a directory: the file the
        // directory resolves to and the pinned length, the shard the room judgment
        // divides, or the refusal either step answers.
        "artifact" => {
            let reference = weaver_types::ArtifactRef(v["path"].as_str().ok_or("path")?.into());
            let resolved = weaver_spu::artifact::resolve(&reference).map_err(|e| format!("{e:?}"))?;
            let pinned = weaver_spu::artifact::pin(&resolved).map_err(|e| format!("{e:?}"))?;
            Ok(json!({"resolved": resolved.file_name().and_then(|n| n.to_str()),
                "len": pinned.len().map_err(|e| format!("{e:?}"))?}))
        },
        // weaver-spu artifact.rs's weights_hash over a directory, after resolve and pin:
        // the canonical walk's blake3, which python-spu's digest must equal whenever
        // nothing is swapped during admission.
        "weights_hash" => {
            let dir = v["path"].as_str().ok_or("path")?;
            let reference = weaver_types::ArtifactRef(dir.into());
            let resolved = weaver_spu::artifact::resolve(&reference).map_err(|e| format!("{e:?}"))?;
            let mut pinned = weaver_spu::artifact::pin(&resolved).map_err(|e| format!("{e:?}"))?;
            Ok(json!(weaver_spu::artifact::weights_hash(std::path::Path::new(dir), &mut pinned).0))
        },
        // weaver-spu artifact.rs's read_header over the resolved and pinned artifact:
        // the container and the family the header names, or the refusal, step two.
        "header" => {
            let reference = weaver_types::ArtifactRef(v["path"].as_str().ok_or("path")?.into());
            let resolved = weaver_spu::artifact::resolve(&reference).map_err(|e| format!("{e:?}"))?;
            let mut pinned = weaver_spu::artifact::pin(&resolved).map_err(|e| format!("{e:?}"))?;
            let header = weaver_spu::artifact::read_header(&mut pinned).map_err(|e| format!("{e:?}"))?;
            Ok(json!({"container": format!("{:?}", header.container), "family": header.family.0,
                "template": header.chat_template}))
        },
        // weaver-spu family/mod.rs's REGISTRY, its families in order.
        "registry_families" => Ok(json!(weaver_spu::family::REGISTRY.iter()
            .map(|declaration| declaration.family).collect::<Vec<_>>())),
        "render" => {
            let messages: Vec<weaver_traits::Message> = serde_json::from_value(body).map_err(|e| e.to_string())?;
            weaver_spu::family::qwen2::renderer().render_identity(&messages)
                .map(|s| json!(s)).map_err(|e| format!("{e:?}"))
        },
        "round" => match v["type"].as_str().unwrap_or("") {
            "TokenDirective" => round::<weaver_types::TokenDirective>(raw_body),
            "TokenAnswer" => round::<weaver_types::TokenAnswer>(raw_body),
            "TokenRefusal" => round::<weaver_types::TokenRefusal>(raw_body),
            "OrganEnvelope" => round::<weaver_types::OrganEnvelope>(raw_body),
            "SpuInstruction" => round::<weaver_types::SpuInstruction>(raw_body),
            "LabelDirective" => round::<weaver_types::LabelDirective>(raw_body),
            "LabelAnswer" => round::<weaver_types::LabelAnswer>(raw_body),
            "LabelRefusal" => round::<weaver_types::LabelRefusal>(raw_body),
            "Generation" => round::<weaver_types::Generation>(raw_body),
            "SegmentPreamble" => round::<weaver_types::SegmentPreamble>(raw_body),
            _ => Err("unknown type".into()),
        },
        _ => Err("unknown operation".into()),
    }
}
fn main() {
    if std::env::args().nth(1).as_deref()==Some("--echo") {
        use std::os::fd::{FromRawFd,OwnedFd};
        let channel=weaver_spu::channel::decode_from_owned(unsafe{OwnedFd::from_raw_fd(3)});
        loop {
            match channel.recv_octets() {
                Ok(body)=>if channel.send_octets(&body).is_err(){std::process::exit(1)},
                Err(weaver_spu::channel::ChannelFault::Closed)=>return,
                Err(_)=>std::process::exit(1),
            }
        }
    }

    for line in io::stdin().lock().lines() {
        #[derive(serde::Deserialize)]
        struct Raw { body: Option<Box<serde_json::value::RawValue>> }
        let result = (|| -> Result<Value,String> {
            let text=line.map_err(|e|e.to_string())?;
            let raw: Raw=serde_json::from_str(&text).map_err(|e|e.to_string())?;
            let value=serde_json::from_str(&text).map_err(|e|e.to_string())?;
            run(value,raw.body.as_ref().map(|b|b.get()).unwrap_or("null"))
        })();
        println!("{}", match result { Ok(v) => json!({"ok": v}), Err(e) => json!({"error": e}) });
    }
}
