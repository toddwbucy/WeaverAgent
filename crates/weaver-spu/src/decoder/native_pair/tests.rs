use super::*;

/// The sharded forward against the stock forward, both halves on one
/// device: isolates the sharding arithmetic from every cross-device
/// question. Skipped where the workshop lacks the artifact or a device.
/// The fixture, by the same variable the device suite reads: unset and
/// absent skips, named and absent fails.
fn fixture_dir() -> Option<std::path::PathBuf> {
    match std::env::var_os("WEAVER_ARTIFACT_QWEN25_SAFETENSORS") {
        Some(named) => {
            let path = std::path::PathBuf::from(named);
            assert!(
                path.is_dir(),
                "WEAVER_ARTIFACT_QWEN25_SAFETENSORS names {}, which is not a directory",
                path.display()
            );
            Some(path)
        }
        None => {
            let path = std::path::PathBuf::from("/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct");
            path.is_dir().then_some(path)
        }
    }
}

#[test]
fn the_halves_recompose_the_whole() {
    let Some(dir) = fixture_dir() else {
        eprintln!("SKIP: no artifact");
        return;
    };
    let dir = dir.as_path();
    if Device::new_cuda(0).is_err() {
        eprintln!("SKIP: no device");
        return;
    }
    let device = Device::new_cuda(0).expect("device");
    let config: Config = serde_json::from_str(
        &std::fs::read_to_string(dir.join("config.json")).expect("config reads"),
    )
    .expect("config parses");
    let container = dir.join("model.safetensors");

    let mut sharded = ShardedModel::load(
        std::slice::from_ref(&container),
        &config,
        [device.clone(), device.clone()],
    )
    .expect("the sharded model loads");
    let vb = unsafe {
        candle_nn::VarBuilder::from_mmaped_safetensors(
            std::slice::from_ref(&container),
            DType::BF16,
            &device,
        )
    }
    .expect("map");
    let mut stock = candle_transformers::models::qwen2::ModelForCausalLM::new(&config, vb)
        .expect("the stock model loads");

    let tokens: Vec<u32> = vec![9707, 11, 1246, 525, 498, 30];
    let sharded_logits = sharded.forward(&tokens, 0, None).expect("sharded forward");
    let input = Tensor::new(tokens.as_slice(), &device)
        .and_then(|t| t.unsqueeze(0))
        .expect("input");
    let stock_logits = stock
        .forward(&input, 0)
        .and_then(|t| t.squeeze(0))
        .and_then(|t| t.squeeze(0))
        .and_then(|t| t.to_dtype(DType::F32))
        .and_then(|t| t.to_vec1::<f32>())
        .expect("stock forward");

    let argmax = |v: &[f32]| {
        v.iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap()
    };
    let max_diff = sharded_logits
        .iter()
        .zip(&stock_logits)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    eprintln!(
        "max logit diff: {max_diff}  argmax sharded {} stock {}",
        argmax(&sharded_logits),
        argmax(&stock_logits)
    );
    assert_eq!(
        argmax(&sharded_logits),
        argmax(&stock_logits),
        "the halves recompose the whole"
    );

    // The same comparison across a real pair, where the workshop holds
    // one, with the failure class printed before the assertion.
    let Ok(second) = Device::new_cuda(1) else {
        eprintln!("SKIP pair half: one device");
        return;
    };
    // The admission's own device walk precedes the load in the engine
    // path, so it precedes it here: the judge's contexts and candle's
    // must coexist or the engine path can never work.
    {
        use weaver_types::DeviceOrdinal;
        let devices = [DeviceOrdinal(0), DeviceOrdinal(1)];
        let _ = crate::gpu::room_and_reach(&devices, 1024, 2 * 1024 * 1024 * 1024);
    }
    let mut paired = ShardedModel::load(
        std::slice::from_ref(&container),
        &config,
        [device.clone(), second],
    )
    .expect("the paired model loads");
    let mut single_ref = ShardedModel::load(
        std::slice::from_ref(&container),
        &config,
        [device.clone(), device.clone()],
    )
    .expect("reference loads");

    // Determinism per variant: each model against itself, twice.
    for (name, model) in [("paired", &mut paired), ("single_ref", &mut single_ref)] {
        model.clear_kv_cache();
        let a = model.forward(&[9707], 0, None).expect("first");
        model.clear_kv_cache();
        let b = model.forward(&[9707], 0, None).expect("second");
        let d = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - y).abs())
            .fold(0f32, f32::max);
        // **Zero, not small: the pin on the measured race.** The cross
        // -device hop unordered against the source stream made the
        // forward nondeterministic, weights exact and every op exact in
        // isolation, and determinism against itself is the cheapest
        // assertion that holds the synchronization in place.
        assert_eq!(d, 0.0, "{name} disagrees with itself, the hop race is back");
        model.clear_kv_cache();
    }

    // One token first: the mask-free path, isolating the seq dimension.
    let one = paired.forward(&[9707], 0, None).expect("one-token forward");
    let one_ref = single_ref
        .forward(&[9707], 0, None)
        .expect("reference forward");
    let one_diff = one
        .iter()
        .zip(&one_ref)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    eprintln!("one-token pair-vs-samedevice max diff: {one_diff}");
    paired.clear_kv_cache();
    let paired_logits = paired.forward(&tokens, 0, None).expect("paired forward");
    let stats = |v: &[f32]| {
        let max = v.iter().fold(f32::MIN, |m, x| m.max(*x));
        let min = v.iter().fold(f32::MAX, |m, x| m.min(*x));
        let nan = v.iter().filter(|x| x.is_nan()).count();
        (min, max, nan)
    };
    eprintln!(
        "pair stats {:?} stock stats {:?} pair head {:?}",
        stats(&paired_logits),
        stats(&stock_logits),
        &paired_logits[..6]
    );
    assert_eq!(
        argmax(&paired_logits),
        argmax(&stock_logits),
        "the pair recomposes the whole"
    );

    // **A multi-token delta after a resident prefix, against the stock
    // model**: the real turn's shape, and the shape whose mask a
    // delta-sized square would misbroadcast. Three tokens resident, then
    // three more, compared at the second call's distribution.
    {
        let split_at = 3;
        paired.clear_kv_cache();
        let _ = paired
            .forward(&tokens[..split_at], 0, None)
            .expect("the prefix decodes");
        let stepped = paired
            .forward(&tokens[split_at..], split_at, None)
            .expect("the delta decodes at its offset");
        // The stock reference walks the delta one token at a time: its
        // own batched delta-at-offset path miscats an F32 mask against a
        // BF16 zero block in the pinned fork, a defect noted for the
        // fork's next touch, and the stepwise walk is the same math
        // through the path everyone exercises.
        stock.clear_kv_cache();
        let input = Tensor::new(&tokens[..split_at], &device)
            .and_then(|t| t.unsqueeze(0))
            .expect("prefix input");
        let _ = stock.forward(&input, 0).expect("stock prefix");
        let mut stock_stepped = Vec::new();
        for (step, token) in tokens[split_at..].iter().enumerate() {
            let input = Tensor::new(&[*token][..], &device)
                .and_then(|t| t.unsqueeze(0))
                .expect("step input");
            stock_stepped = stock
                .forward(&input, split_at + step)
                .and_then(|t| t.squeeze(0))
                .and_then(|t| t.squeeze(0))
                .and_then(|t| t.to_dtype(DType::F32))
                .and_then(|t| t.to_vec1::<f32>())
                .expect("stock step");
        }
        assert_eq!(
            argmax(&stepped),
            argmax(&stock_stepped),
            "a delta at an offset recomposes the stock model's answer"
        );
        paired.clear_kv_cache();
    }

    // The engine's route: the pinned descriptor path instead of the
    // name. The admission hands the loader /proc/self/fd/N, so the
    // pair loader must read the same bytes through it.
    use std::os::fd::AsRawFd;
    let held = std::fs::File::open(&container).expect("the container opens");
    let fd_path = std::path::PathBuf::from(format!("/proc/self/fd/{}", held.as_raw_fd()));
    let mut via_fd = ShardedModel::load(
        std::slice::from_ref(&fd_path),
        &config,
        [device.clone(), Device::new_cuda(1).unwrap()],
    )
    .expect("the descriptor-path load succeeds");
    let fd_logits = via_fd
        .forward(&tokens, 0, None)
        .expect("descriptor-path forward");
    let fd_diff = fd_logits
        .iter()
        .zip(&paired_logits)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    eprintln!("descriptor-path vs name-path: max diff {fd_diff}");
    assert_eq!(
        argmax(&fd_logits),
        argmax(&stock_logits),
        "the descriptor path reads the same bytes"
    );

    // The engine's other difference: it decodes against a clone of the
    // resident's model, per the session discipline.
    let mut cloned = via_fd.session_clone();
    let clone_logits = cloned
        .forward(&tokens, 0, None)
        .expect("the clone forwards");
    eprintln!(
        "clone argmax {} original argmax {}",
        argmax(&clone_logits),
        argmax(&stock_logits)
    );
    assert_eq!(
        argmax(&clone_logits),
        argmax(&stock_logits),
        "a session clone computes what the resident computes"
    );
}
