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
