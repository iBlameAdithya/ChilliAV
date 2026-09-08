use chilli_context::builder::ContextBuilder;
use chilli_repo_intel::repo_intel::CodeGraph;

#[test]
fn test_context_builder_includes_codegraph_symbols() {
    let mut builder = ContextBuilder::new(100_000);
    let mut graph = CodeGraph::open_in_memory().unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let ts_file = temp_dir.path().join("auth.ts");
    std::fs::write(
        &ts_file,
        "export class AuthService { login(user: string) { return true; } }",
    )
    .unwrap();
    graph.index_file(&ts_file).unwrap();

    builder.add_symbol_graph_context(&graph, "Refactor AuthService login flow");
    let ctx = builder.build().unwrap();
    assert!(
        ctx.system_prompt.contains("Symbol Context")
            || ctx
                .messages
                .iter()
                .any(|m| m.content.contains("AuthService"))
    );
}
