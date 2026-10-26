//! Repeatable public-command workflow. See docs/automation.md.
#[path = "support/live_effects_workflow.rs"]
mod live_effects_workflow;
#[path = "support/workflow.rs"]
mod workflow;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let output = value("--out").expect("--out NEW_DIRECTORY is required");
    let kind = value("--adapter").unwrap_or("direct".into());
    let mut adapter = workflow::Adapter::connect(
        &kind,
        value("--binary").as_deref(),
        value("--attach").as_deref(),
        value("--runtime-dir").as_deref(),
    )
    .await;
    let mut summary = if args.iter().any(|arg| arg == "--live-effects") {
        assert!(
            !std::path::Path::new(&output).exists(),
            "Use a new output directory"
        );
        live_effects_workflow::run(
            &mut adapter,
            std::path::Path::new(&output),
            value("--attach").is_some(),
        )
        .await
    } else {
        workflow::run(
            &mut adapter,
            std::path::Path::new(&output),
            args.iter().any(|arg| arg == "--audition"),
        )
        .await
    };
    summary.as_object_mut().unwrap().remove("mixed_pcm");
    summary.as_object_mut().unwrap().remove("samples");
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    adapter.shutdown().await;
}
