pub mod input;
pub mod physics;
pub mod render;
pub mod viewport;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use fdg_sim::petgraph::graph::NodeIndex;
use fdg_sim::{ForceGraph, ForceGraphHelper, Simulation, SimulationParameters};

use crate::config::GrafConfig;
use crate::linker::{FileData, resolve_links};

pub struct GraphNodeData {
    pub relative_path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub link_count: usize,
    pub folder: String,
}

fn folder_from_path(relative_path: &str) -> String {
    Path::new(relative_path)
        .parent()
        .and_then(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .unwrap_or_else(|| "(root)".to_string())
}

pub struct GraphState {
    pub simulation: Simulation<GraphNodeData, ()>,
    pub viewport: viewport::Viewport,
    pub selected_node: Option<NodeIndex>,
    pub dragging_node: Option<NodeIndex>,
    pub drag_target: Option<(f32, f32)>,
    pub is_settled: bool,
    pub graph_bounds: (f64, f64, f64, f64),
    pub render_cache: Mutex<render::RenderCache>,
}

impl GraphState {
    pub fn new(files: &[FileData], config: &GrafConfig) -> Self {
        let graph = build_graph(files, config);
        let simulation = create_simulation(graph, config);
        let mut state = Self {
            viewport: viewport::Viewport::default(),
            simulation,
            selected_node: None,
            dragging_node: None,
            drag_target: None,
            is_settled: false,
            graph_bounds: (0.0, 0.0, 0.0, 0.0),
            render_cache: Mutex::new(render::RenderCache::new()),
        };
        state.viewport = state.viewport.auto_fit_from_graph(
            state.simulation.get_graph(),
            config.interaction.auto_fit_padding,
        );
        state.graph_bounds = render::compute_graph_bounds(state.simulation.get_graph());
        state
    }
}

pub fn build_graph(files: &[FileData], config: &GrafConfig) -> ForceGraph<GraphNodeData, ()> {
    let links = resolve_links(files, &config.filter.exclude_tags);
    let mut graph: ForceGraph<GraphNodeData, ()> = ForceGraph::default();
    let mut path_to_index: HashMap<String, NodeIndex> = HashMap::new();

    let filtered: Vec<&FileData> = files
        .iter()
        .filter(|f| {
            config.filter.exclude_tags.is_empty()
                || f.tags
                    .iter()
                    .all(|t| !config.filter.exclude_tags.contains(t))
        })
        .filter(|f| {
            let lc = links.get(&f.relative_path).map(|v| v.len()).unwrap_or(0);
            lc >= config.filter.min_links
        })
        .collect();

    for file in &filtered {
        let lc = links.get(&file.relative_path).map(|v| v.len()).unwrap_or(0);
        let data = GraphNodeData {
            relative_path: file.relative_path.clone(),
            title: file.title.clone(),
            tags: file.tags.clone(),
            link_count: lc,
            folder: folder_from_path(&file.relative_path),
        };
        let idx = graph.add_force_node(&file.relative_path, data);
        path_to_index.insert(file.relative_path.clone(), idx);
    }

    let mut edge_set: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
    for (source, targets) in &links {
        if let Some(&source_idx) = path_to_index.get(source) {
            for target in targets {
                if let Some(&target_idx) = path_to_index.get(target)
                    && source_idx != target_idx
                    && edge_set.insert((source_idx, target_idx))
                {
                    graph.add_edge(source_idx, target_idx, ());
                }
            }
        }
    }

    graph
}

pub fn search_nodes(
    sim: &Simulation<GraphNodeData, ()>,
    query: &str,
    max_results: usize,
) -> Vec<(NodeIndex, String)> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_lowercase();
    let graph = sim.get_graph();
    let mut results: Vec<(NodeIndex, String)> = graph
        .node_indices()
        .filter_map(|idx| {
            let node = &graph[idx];
            let title_match = node.data.title.to_lowercase().contains(&q);
            let path_match = node.data.relative_path.to_lowercase().contains(&q);
            let tag_match = node.data.tags.iter().any(|t| t.to_lowercase().contains(&q));
            if title_match || path_match || tag_match {
                Some((idx, node.data.title.clone()))
            } else {
                None
            }
        })
        .collect();
    results.sort_by_key(|a| a.1.to_lowercase());
    results.truncate(max_results);
    results
}

pub fn create_simulation(
    graph: ForceGraph<GraphNodeData, ()>,
    config: &GrafConfig,
) -> Simulation<GraphNodeData, ()> {
    let force = fdg_sim::force::handy(
        config.physics.ideal_distance as f32,
        config.physics.damping,
        config.physics.cooling,
        config.physics.prevent_overlapping,
    );
    let params = SimulationParameters::new(
        config.physics.max_iterations as f32,
        fdg_sim::Dimensions::Two,
        force,
    );
    Simulation::from_graph(graph, params)
}
