use serde_json::json;
#[path = "../examples/support/workflow.rs"]
mod workflow;
#[tokio::test]
async fn complete_workflow_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let mut summaries = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let summary = workflow::run(&mut adapter, &directory.path().join(kind), false).await;
        adapter.shutdown().await;
        summaries.push(summary);
    }
    assert_eq!(summaries[0], summaries[1]);
    assert_eq!(summaries[0], summaries[2]);
    for name in ["Mix.wav", "Mix.mp3", "Sample selection.mp3"] {
        let expected = std::fs::read(directory.path().join("direct").join(name)).unwrap();
        for kind in ["headless", "mcp"] {
            assert_eq!(
                expected,
                std::fs::read(directory.path().join(kind).join(name)).unwrap(),
                "{kind} {name}"
            );
        }
    }
}

#[tokio::test]
async fn soft_clipper_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Soft Clipper.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"soft_clipper"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn reverb_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Reverb.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"reverb"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":4096})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn parametric_eq_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Parametric EQ.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"parametric_eq"}})).await;
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":state["committed"]["effects"][0]["id"],"parameter_id":"level4","value":1200}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":4096})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn fast_distortion_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Fast Distortion.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"fast_distortion"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn delay_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Delay.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 8000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"delay"}})).await;
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        let id = state["committed"]["effects"][0]["id"].clone();
        for (key, value) in [
            ("tempo_sync", json!(false)),
            ("time", json!(1)),
            ("dry", json!(0)),
        ] {
            let state = adapter.ok("processing.get", json!({"owner":owner})).await;
            adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"set_parameter","effect_id":id,"parameter_id":key,"value":value}})).await;
        }
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn chorus_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Chorus.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"chorus"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn limiter_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Limiter.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"limiter"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn balance_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Balance.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"balance"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn stereo_enhancer_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Stereo Enhancer.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"stereo_enhancer"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn stereo_shaper_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Stereo Shaper.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"stereo_shaper"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[tokio::test]
async fn transient_processor_render_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Transient Processor.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for frame in 0..256 {
        let value = (frame as f32 * 0.157).sin() * 1.5;
        writer.write_sample(value).unwrap();
        writer.write_sample(value * 0.731).unwrap();
    }
    writer.finalize().unwrap();
    let mut outputs = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        let document = adapter.job("documents.open", json!({"path":path})).await["result"].clone();
        let owner = json!({"kind":"document","id":document["id"]});
        let state = adapter.ok("processing.get", json!({"owner":owner})).await;
        adapter.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":state["committed"]["edit_revision_id"],"change":{"action":"add","kind":"transient_processor"}})).await;
        let render = adapter
            .job("processing.render", json!({"owner":owner}))
            .await["output"]["details"]
            .clone();
        let samples=adapter.ok("processing.samples",json!({"audio_revision_id":render["audio_revision_id"],"start_frame":0,"frames":256})).await;
        outputs.push(samples["channels"].clone());
        adapter.shutdown().await;
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
}

#[path = "../examples/support/live_effects_workflow.rs"]
mod live_effects_workflow;
#[tokio::test]
async fn live_effects_portable_workflow_agrees_across_direct_headless_and_mcp() {
    let directory = tempfile::tempdir().unwrap();
    let mut results = Vec::new();
    for (kind, binary) in [
        ("direct", None),
        ("headless", Some(env!("CARGO_BIN_EXE_wavebob-headless"))),
        ("mcp", Some(env!("CARGO_BIN_EXE_wavebob-mcp"))),
    ] {
        let mut adapter = workflow::Adapter::connect(kind, binary, None, None).await;
        results.push(
            live_effects_workflow::run(&mut adapter, &directory.path().join(kind), false).await,
        );
        adapter.shutdown().await;
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0], results[2]);
    for name in ["Live.wav", "Live.mp3"] {
        assert_eq!(
            std::fs::read(directory.path().join("direct").join(name)).unwrap(),
            std::fs::read(directory.path().join("headless").join(name)).unwrap()
        );
        assert_eq!(
            std::fs::read(directory.path().join("direct").join(name)).unwrap(),
            std::fs::read(directory.path().join("mcp").join(name)).unwrap()
        );
    }
    if let Some(root) = std::env::var_os("WAVEBOB_TEST_ARTIFACT_DIR") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(
            directory.path().join("direct/workflow.json"),
            std::path::Path::new(&root).join("adapter-workflow.json"),
        )
        .unwrap();
    }
}
