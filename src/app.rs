use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;

use fdg_sim::petgraph::graph::NodeIndex;

use crate::config::GrafConfig;
use crate::input::GraphMouseState;
use crate::linker::FileData;

pub struct AppState {
    pub graph_state: Option<Arc<RwLock<crate::graph::GraphState>>>,
    pub graph_kill_tx: Option<std::sync::mpsc::Sender<()>>,
    pub keymap: crate::input::GraphKeymap,
    pub graph_mouse_state: GraphMouseState,
    pub base_dir: PathBuf,
    pub focus_note_ids: Option<HashSet<String>>,
    pub files: Vec<FileData>,
    pub show_help: bool,
    pub config_errors: Vec<String>,
    pub search_active: bool,
    pub search_query: String,
    pub search_results: Vec<(NodeIndex, String)>,
    pub search_selected: usize,
    pub search_cursor: usize,
    pub show_legend: bool,
    pub show_minimap: bool,
    pub show_grid: bool,
    pub show_status_bar: bool,
    pub config_reload_msg: Option<String>,
    pub config_reload_ttl: u16,
}

fn node_specs(files: &[FileData]) -> Vec<crate::graph::NodeSpec> {
    files
        .iter()
        .map(|f| crate::graph::NodeSpec {
            id: f.relative_path.clone(),
            title: f.title.clone(),
            tags: f.tags.clone(),
            folder: std::path::PathBuf::from(&f.relative_path)
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            links: f.wikilinks.clone(),
        })
        .collect()
}

impl AppState {
    pub fn new(
        config: &GrafConfig,
        base_dir: PathBuf,
        files: Vec<FileData>,
        config_errors: Vec<String>,
    ) -> Self {
        let specs = node_specs(&files);
        let graph_state =
            crate::graph::GraphState::from_specs(&specs, config).expect("Failed to build graph");
        let state = Arc::new(RwLock::new(graph_state));
        let kill_tx = crate::physics::start_physics(state.clone(), config);

        Self {
            graph_state: Some(state),
            graph_kill_tx: kill_tx,
            graph_mouse_state: GraphMouseState::default(),
            base_dir,
            keymap: crate::input::GraphKeymap::default(),
            focus_note_ids: None,
            files,
            show_help: false,
            config_errors,
            search_active: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            search_cursor: 0,
            show_minimap: config.visual.show_minimap,
            show_legend: config.visual.show_legend,
            show_grid: config.visual.show_grid,
            show_status_bar: config.display.show_status_bar,
            config_reload_msg: None,
            config_reload_ttl: 0,
        }
    }

    pub fn refresh_simulation(&mut self, config: &GrafConfig) {
        if let Some(kill_tx) = self.graph_kill_tx.take() {
            let _ = kill_tx.send(());
        }
        // Focus (local/group) subsets must render every selected node,
        // including ones without connections, regardless of show_orphan.
        let mut effective = config.clone();
        if self.focus_note_ids.is_some() {
            effective.filter.show_orphan = true;
        }
        let files: Vec<FileData> = match &self.focus_note_ids {
            Some(ids) => self
                .files
                .iter()
                .filter(|f| ids.contains(&f.relative_path))
                .cloned()
                .collect(),
            None => self.files.clone(),
        };
        let specs = node_specs(&files);
        let graph_state = crate::graph::GraphState::from_specs(&specs, &effective)
            .expect("Failed to build graph");
        let state = Arc::new(RwLock::new(graph_state));
        let kill_tx = crate::physics::start_physics(state.clone(), &effective);
        self.graph_state = Some(state);
        self.graph_kill_tx = kill_tx;
        // Clear search state — old NodeIndex values are invalid in the new graph
        self.search_results.clear();
        self.search_selected = 0;
    }

    pub fn enter_focus(
        &mut self,
        config: &GrafConfig,
        ids: HashSet<String>,
        mode: crate::graph::ModeBanner,
    ) {
        self.focus_note_ids = Some(ids);
        self.refresh_simulation(config);
        if let Some(gs) = &self.graph_state {
            gs.write().mode_banner = Some(mode);
        }
    }

    pub fn exit_focus(&mut self, config: &GrafConfig) {
        self.focus_note_ids = None;
        self.refresh_simulation(config);
    }

    pub fn shutdown(&mut self) {
        if let Some(kill_tx) = self.graph_kill_tx.take() {
            let _ = kill_tx.send(());
        }
        self.graph_state = None;
    }
}
