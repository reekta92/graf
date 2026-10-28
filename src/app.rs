use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use fdg_sim::petgraph::graph::NodeIndex;

use crate::config::GrafConfig;
use crate::document::{Note, resolve_note_links};
use crate::input::GraphMouseState;

pub struct AppState {
    pub graph_state: Option<Arc<RwLock<crate::graph::GraphState>>>,
    pub graph_kill_tx: Option<std::sync::mpsc::Sender<()>>,
    pub keymap: crate::input::GraphKeymap,
    pub graph_mouse_state: GraphMouseState,
    pub base_dir: PathBuf,
    pub focus_note_ids: Option<HashSet<String>>,
    pub files: Vec<Note>,
    pub scan_errors: Vec<String>,
    pub link_errors: Vec<String>,
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
    /// Viewport to compare against: auto-fit again once physics settles, unless
    /// the user already panned or zoomed away from this.
    pub auto_fit_baseline: Option<(f64, f64, f64)>,
}

fn node_specs(files: &[Note]) -> Vec<crate::graph::NodeSpec> {
    files
        .iter()
        .map(|note| {
            let f = &note.data;
            crate::graph::NodeSpec {
                id: f.relative_path.clone(),
                title: f.title.clone(),
                tags: f.tags.clone(),
                folder: std::path::PathBuf::from(&f.relative_path)
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                links: f.wikilinks.clone(),
            }
        })
        .collect()
}

impl AppState {
    pub fn new(
        config: &GrafConfig,
        base_dir: PathBuf,
        files: Vec<Note>,
        scan_errors: Vec<String>,
        config_errors: Vec<String>,
    ) -> Self {
        let specs = node_specs(&files);
        let (links, link_errors) = resolve_note_links(&files, config.parsing.link_resolution);
        let graph_state = crate::graph::GraphState::from_resolved_specs(&specs, &links, config)
            .expect("Failed to build graph");
        let state = Arc::new(RwLock::new(graph_state));
        let kill_tx = crate::physics::start_physics(state.clone(), config);
        let auto_fit_baseline = {
            let mut guard = state.write();
            let vp = guard
                .viewport
                .auto_fit_from_graph(guard.simulation.get_graph(), 1.4);
            guard.viewport = vp;
            Some((
                guard.viewport.center_x,
                guard.viewport.center_y,
                guard.viewport.zoom,
            ))
        };

        Self {
            graph_state: Some(state),
            graph_kill_tx: kill_tx,
            graph_mouse_state: GraphMouseState::default(),
            base_dir,
            keymap: crate::input::GraphKeymap::default(),
            focus_note_ids: None,
            files,
            scan_errors,
            link_errors,
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
            auto_fit_baseline,
        }
    }

    /// Per-frame: fit the viewport to the graph once physics settles, unless the
    /// user already moved the camera.
    pub fn tick(&mut self) {
        if let (Some(baseline), Some(state)) = (self.auto_fit_baseline, &self.graph_state) {
            let mut guard = state.write();
            let vp = &guard.viewport;
            if (vp.center_x, vp.center_y, vp.zoom) != baseline {
                self.auto_fit_baseline = None;
            } else if guard.is_settled {
                let fitted = vp.auto_fit_from_graph(guard.simulation.get_graph(), 1.4);
                guard.viewport = fitted;
                self.auto_fit_baseline = None;
            }
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
        let (links, errors) = resolve_note_links(&self.files, config.parsing.link_resolution);
        self.link_errors = errors;
        let mut specs = node_specs(&self.files);
        if let Some(ids) = &self.focus_note_ids {
            specs.retain(|n| ids.contains(&n.id));
        }
        let graph_state = crate::graph::GraphState::from_resolved_specs(&specs, &links, &effective)
            .expect("Failed to build graph");
        let state = Arc::new(RwLock::new(graph_state));
        let kill_tx = crate::physics::start_physics(state.clone(), &effective);
        self.graph_state = Some(state);
        self.graph_kill_tx = kill_tx;
        // Clear search state — old NodeIndex values are invalid in the new graph
        self.search_results.clear();
        self.search_selected = 0;
    }

    pub fn rescan(&mut self, config: &GrafConfig) {
        let (files, errors) = crate::document::scan_notes(
            &self.base_dir,
            &config.filter.exclude_patterns,
            &config.parsing,
        );
        self.files = files;
        self.scan_errors = errors;
        self.refresh_simulation(config);
    }

    pub fn edit_connection(
        &mut self,
        config: &GrafConfig,
        source: &str,
        target: &str,
        create: bool,
    ) -> anyhow::Result<()> {
        let pairs = if create {
            vec![(source, target)]
        } else {
            vec![(source, target), (target, source)]
        };
        for (source, target) in pairs {
            let index = self
                .files
                .iter()
                .position(|n| n.data.relative_path == source)
                .ok_or_else(|| anyhow::anyhow!("source note no longer exists"))?;
            let path = self.base_dir.join(source);
            let content = std::fs::read_to_string(&path)?;
            let updated = crate::document::edit_connection(
                &content,
                source,
                target,
                &self.files,
                &config.parsing,
                create,
            )?;
            if updated != content {
                crate::document::save_note(&self.base_dir, source, &content, &updated)?;
            }
            self.files[index] = crate::document::parse_note(&updated, source, &config.parsing)?;
        }
        let (links, errors) = resolve_note_links(&self.files, config.parsing.link_resolution);
        self.link_errors = errors;
        let connected = links
            .get(source)
            .is_some_and(|ls| ls.iter().any(|l| l == target))
            || links
                .get(target)
                .is_some_and(|ls| ls.iter().any(|l| l == source));
        if let Some(gs) = &self.graph_state {
            let mut guard = gs.write();
            let graph = guard.simulation.get_graph();
            let src = graph.node_indices().find(|&i| graph[i].data.id == source);
            let tgt = graph.node_indices().find(|&i| graph[i].data.id == target);
            if let (Some(src), Some(tgt)) = (src, tgt) {
                crate::graph::apply_connection_change(&mut guard.simulation, src, tgt, connected);
                let mut cache = guard.render_cache.lock();
                cache.topology_dirty = true;
                cache.minimap_dirty = true;
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{LinkFormat, LinkResolution, scan_notes, test_support::Notebook};

    fn app(notebook: &Notebook) -> (AppState, GrafConfig) {
        let mut config = GrafConfig::default();
        config.filter.show_orphan = true;
        config.parsing.link_resolution = LinkResolution::FilenameFirst;
        let (notes, errors) = scan_notes(&notebook.0, &[], &config.parsing);
        let state = AppState::new(&config, notebook.0.clone(), notes, errors, vec![]);
        (state, config)
    }

    fn edges(app: &AppState) -> usize {
        app.graph_state
            .as_ref()
            .unwrap()
            .read()
            .simulation
            .get_graph()
            .edge_count()
    }

    #[test]
    fn create_delete_and_rescan_match_disk_with_duplicate_titles() {
        let notebook = Notebook::new();
        notebook.write("source.md", "# Source\nBody.\n");
        notebook.write("a.md", "# todo\n[[source]]\n");
        notebook.write("b.md", "# todo\n");
        let (mut app, mut config) = app(&notebook);
        assert_eq!(edges(&app), 1);
        config.parsing.link_format = LinkFormat::Markdown;
        app.edit_connection(&config, "source.md", "a.md", true)
            .unwrap();
        assert!(
            std::fs::read_to_string(notebook.0.join("source.md"))
                .unwrap()
                .contains("[todo](a.md)")
        );
        assert_eq!(edges(&app), 1);
        app.rescan(&config);
        assert_eq!(edges(&app), 1);
        app.edit_connection(&config, "source.md", "a.md", false)
            .unwrap();
        assert_eq!(edges(&app), 0);
        app.rescan(&config);
        assert_eq!(edges(&app), 0);
        assert_eq!(
            std::fs::read_to_string(notebook.0.join("b.md")).unwrap(),
            "# todo\n"
        );
        app.shutdown();
    }

    #[test]
    fn rescan_reloads_files_and_inline_tag_settings() {
        let notebook = Notebook::new();
        notebook.write("a.md", "# A\n#private\n");
        let (mut app, mut config) = app(&notebook);
        assert!(app.files[0].data.tags.is_empty());
        notebook.write("b.md", "# B\n[[a]]\n");
        config.parsing.hashtags = true;
        app.rescan(&config);
        assert_eq!(app.files.len(), 2);
        assert_eq!(app.files[0].data.tags, ["private"]);
        assert_eq!(edges(&app), 1);
        config.filter.exclude_tags.push("private".into());
        app.rescan(&config);
        assert_eq!(edges(&app), 0);
        app.shutdown();
    }

    #[test]
    fn focused_views_do_not_disambiguate_a_notebook_link() {
        let notebook = Notebook::new();
        notebook.write("source.md", "# Source\n[[todo]]\n");
        notebook.write("a.md", "# todo\n");
        notebook.write("b.md", "# todo\n");
        let (mut app, config) = app(&notebook);
        assert_eq!(edges(&app), 0);
        assert!(!app.link_errors.is_empty());
        app.focus_note_ids = Some(HashSet::from(["source.md".into(), "a.md".into()]));
        app.refresh_simulation(&config);
        assert_eq!(edges(&app), 0);
        assert!(!app.link_errors.is_empty());
        app.shutdown();
    }

    #[cfg(unix)]
    #[test]
    fn failed_save_leaves_memory_and_disk_unchanged() {
        let notebook = Notebook::new();
        notebook.write("source.md", "# Source\n");
        notebook.write("a.md", "# A\n");
        let (mut app, config) = app(&notebook);
        let path = notebook.0.join("source.md");
        let permissions = std::fs::metadata(&path).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&path, readonly).unwrap();
        assert!(
            app.edit_connection(&config, "source.md", "a.md", true)
                .is_err()
        );
        assert_eq!(edges(&app), 0);
        assert!(app.files.iter().all(|n| n.links.is_empty()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Source\n");
        std::fs::set_permissions(path, permissions).unwrap();
        app.shutdown();
    }
}
