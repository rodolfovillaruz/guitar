use crate::{
    app::{
        app::{App, Focus},
        state::layout::Layout,
    },
    core::graph_service::{GraphCommand, GraphLookupKind},
};
use git2::Repository;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_repo(name: &str) -> (PathBuf, Repository) {
    let id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("guitar-inspector-draw-{name}-{id}"));
    fs::create_dir_all(&path).unwrap();
    let repo = Repository::init_opts(&path, git2::RepositoryInitOptions::new().initial_head("master")).unwrap();
    (path, repo)
}

fn rendered(terminal: &Terminal<TestBackend>) -> String {
    terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect::<String>()
}

#[test]
fn inspector_loading_requests_missing_graph_row() {
    let (_path, repo) = temp_repo("loading");
    let (tx, rx) = std::sync::mpsc::channel();
    let mut app = App {
        focus: Focus::Inspector,
        graph_selected: 42,
        graph_tx: Some(tx),
        layout: Layout { inspector: Rect::new(0, 0, 48, 5), inspector_scrollbar: Rect::new(47, 0, 1, 5), ..Default::default() },
        ..Default::default()
    };
    app.graph.generation = 7;
    app.layout_config.is_zen = false;

    let backend = TestBackend::new(48, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.draw_inspector(frame, &repo)).unwrap();

    let rendered = rendered(&terminal);
    assert!(rendered.contains("loading"), "{rendered}");

    match rx.try_recv().unwrap() {
        GraphCommand::Lookup { generation, request_id, kind: GraphLookupKind::GraphRowAt { index } } => {
            assert_eq!(generation, 7);
            assert_eq!(request_id, 1);
            assert_eq!(index, 42);
        },
        other => panic!("expected graph row lookup, got {other:?}"),
    }
}
