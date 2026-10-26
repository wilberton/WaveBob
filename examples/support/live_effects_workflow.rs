//! Reproducible delivered live-effects scenario through the public adapter seam.
use crate::workflow::Adapter;
use serde_json::{Value, json};
use std::path::Path;
async fn edit(a: &mut Adapter, owner: &Value, change: Value) -> Value {
    let p = a.ok("processing.get", json!({"owner":owner})).await;
    a.ok("processing.edit",json!({"owner":owner,"expected_edit_revision":p["committed"]["edit_revision_id"],"change":change})).await
}
pub async fn run(a: &mut Adapter, root: &Path, native: bool) -> Value {
    std::fs::create_dir_all(root).unwrap();
    let path = root.join("Live source.wav");
    let mut w = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for f in 0..6000 {
        w.write_sample((f as f32 * 0.071).sin() * 0.15).unwrap();
        w.write_sample(if f == 1000 { 0.3f32 } else { 0.0 })
            .unwrap();
    }
    w.finalize().unwrap();
    let d = a.job("documents.open", json!({"path":path})).await["result"].clone();
    let o = json!({"kind":"document","id":d["id"]});
    let catalogue = a.ok("processing.catalogue", json!({})).await;
    assert_eq!(catalogue.as_array().unwrap().len(), 11);
    assert!(
        catalogue
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["available"] == true)
    );
    let ar = a
        .ok(
            "tracks.add",
            json!({"name":"Linked bodies and preserved tails"}),
        )
        .await;
    let t = ar["tracks"][0]["id"].clone();
    for (start, end, at) in [(0, 6000, 0), (0, 6000, 8000), (1000, 4000, 16000)] {
        a.ok("clips.insert",json!({"track_id":t,"document_id":d["id"],"revision_id":d["revision_id"],"source_start":start,"source_end":end,"position":at})).await;
    }
    let mp3 = root.join("Original MP3.mp3");
    a.job("audio.export",json!({"target":{"scope":"whole","document_id":d["id"],"revision_id":d["revision_id"]},"path":mp3,"format":{"kind":"mp3","bitrate_kbps":192}})).await;
    let other = a.job("documents.open", json!({"path":mp3})).await["result"].clone();
    a.ok("documents.activate", json!({"document_id":d["id"]}))
        .await;
    if native {
        a.ok("presentation.set_mode", json!({"mode":"waveform"}))
            .await;
    }
    let mut ids = Vec::new();
    for kind in ["balance", "soft_clipper", "stereo_enhancer"] {
        let p = edit(a, &o, json!({"action":"add","kind":kind})).await;
        ids.push(
            p["committed"]["effects"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["id"]
                .clone(),
        );
    }
    edit(
        a,
        &o,
        json!({"action":"set_parameter","effect_id":ids[2],"parameter_id":"phase","value":512}),
    )
    .await;
    a.ok("transport.loop.set", json!({"enabled":true})).await;
    if native {
        let result = a
            .raw("transport.play", json!({"document_id":d["id"]}))
            .await;
        std::fs::write(
            root.join("native-playback.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
    let p = a.ok("processing.get", json!({"owner":o})).await;
    let p = a
        .ok(
            "processing.preview.begin",
            json!({"owner":o,"expected_edit_revision":p["committed"]["edit_revision_id"]}),
        )
        .await;
    a.ok("processing.preview.update",json!({"preview_id":p["preview_id"],"change":{"action":"set_envelope","envelope":{"gain_db":-3.0,"fade_in_frames":500,"fade_out_frames":700}}})).await;
    a.ok(
        "processing.preview.commit",
        json!({"preview_id":p["preview_id"]}),
    )
    .await;
    edit(a, &o, json!({"action":"move","effect_id":ids[1],"index":0})).await;
    edit(
        a,
        &o,
        json!({"action":"bypass","effect_id":ids[0],"bypassed":true}),
    )
    .await;
    a.ok("history.undo", json!({})).await;
    let mut editor_views = Vec::new();
    if native {
        a.ok("transport.pause", json!({})).await;
        for id in &ids {
            a.ok("effects.editor.open", json!({"owner":o,"instance_id":id}))
                .await;
        }
        let view = a.ok("presentation.get", json!({})).await;
        assert_eq!(view["editor_windows"].as_array().unwrap().len(), 3);
        for (i, window) in view["editor_windows"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            eprintln!("Capturing simultaneous editor {i}");
            a.ok(
                "effects.editor.focus",
                json!({"owner":o,"instance_id":window["instance_id"]}),
            )
            .await;
            let result=a.ok("capture.png",json!({"editor_id":window["id"],"path":root.join(format!("effect-{i}.png")),"timeout_ms":15000})).await;
            assert_eq!(result["metadata"]["owner"], o);
            assert_eq!(result["metadata"]["instance_id"], window["instance_id"]);
            editor_views.push(result["metadata"].clone());
        }
        a.capture(&root.join("waveform.png")).await;
        a.ok("transport.pause", json!({})).await;
        a.ok("presentation.set_mode", json!({"mode":"multitrack"}))
            .await;
    }
    let ar = a.ok("arrangement.get", json!({})).await;
    a.job("mix.prepare", json!({"revision_id":ar["revision_id"]}))
        .await;
    let ar = a.ok("arrangement.get", json!({})).await;
    let clips = ar["tracks"][0]["clips"].as_array().unwrap();
    assert_eq!(
        clips[0]["bounce"]["audio_revision_id"],
        clips[1]["bounce"]["audio_revision_id"]
    );
    assert_ne!(
        clips[1]["bounce"]["audio_revision_id"],
        clips[2]["bounce"]["audio_revision_id"]
    );
    assert!(clips[0]["tail_frames"].as_u64().unwrap() > 8000);
    let co = json!({"kind":"clip","id":clips[1]["id"]});
    edit(a,&co,json!({"action":"set_envelope","envelope":{"gain_db":-6.0,"fade_in_frames":300,"fade_out_frames":500}})).await;
    edit(a, &co, json!({"action":"add","kind":"limiter"})).await;
    let ar = a.ok("arrangement.get", json!({})).await;
    a.job("mix.prepare", json!({"revision_id":ar["revision_id"]}))
        .await;
    let ar = a.ok("arrangement.get", json!({})).await;
    assert_eq!(ar["tracks"][0]["clips"][1]["position"], 8000);
    assert_eq!(ar["tracks"][0]["clips"][2]["position"], 16000);
    if native {
        a.ok(
            "presentation.timeline.set",
            json!({"start_frame":0,"end_frame":50000}),
        )
        .await;
        a.capture(&root.join("multitrack.png")).await;
    }
    let before = a
        .ok(
            "mix.samples",
            json!({"revision_id":ar["revision_id"],"start_frame":0,"frames":4096}),
        )
        .await["channels"]
        .clone();
    let project = root.join("Live.wavebob");
    a.job("project.save", json!({"path":project})).await;
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(mp3).unwrap();
    // Reopening in the same session replaces all source/cache ownership; portable
    // tests separately reopen with an entirely fresh Session.
    a.job(
        "project.open",
        json!({"path":project,"discard_changes":true}),
    )
    .await;
    let ar = a.ok("arrangement.get", json!({})).await;
    a.job("mix.prepare", json!({"revision_id":ar["revision_id"]}))
        .await;
    let after = a
        .ok(
            "mix.samples",
            json!({"revision_id":ar["revision_id"],"start_frame":0,"frames":4096}),
        )
        .await["channels"]
        .clone();
    assert_eq!(before, after);
    let mut outputs = Vec::new();
    for (name, format) in [
        ("Live.wav", json!({"kind":"wav","encoding":"float32"})),
        ("Live.mp3", json!({"kind":"mp3","bitrate_kbps":192})),
    ] {
        let result=a.job("mix.export",json!({"revision_id":ar["revision_id"],"start_frame":0,"end_frame":19000,"include_tail":true,"path":root.join(name),"format":format})).await;
        let detail = &result["output"]["details"];
        outputs.push(json!({"frames":detail["frames"],"channels":detail["channels"],"source_frames":detail["source_frames"]}));
    }
    let result = json!({"catalogue_count":11,"source_files_removed":true,"samples":before,"outputs":outputs,"positions":[0,8000,16000],"mp3_origin_frames":other["frames"]});
    std::fs::write(root.join("workflow.json"),serde_json::to_vec_pretty(&json!({"result":result,"native_editors":editor_views,"physical_listening":"unverified"})).unwrap()).unwrap();
    result
}
