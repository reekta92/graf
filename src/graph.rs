use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};

use fdg_sim::petgraph::graph::NodeIndex;
use fdg_sim::{ForceGraph, ForceGraphHelper, Simulation, SimulationParameters};

use crate::settings::Settings;

// ── Generic Selection Type ───────────────────────────────────────────────────

use std::hash::Hash;

#[derive(Debug, Clone, Default)]
pub struct Selection<Id: Eq + Hash + Clone> {
    pub primary: Option<Id>,
    pub extra: HashSet<Id>,
}

impl<Id: Eq + Hash + Clone> Selection<Id> {
    pub fn new() -> Self {
        Self {
            primary: None,
            extra: HashSet::new(),
        }
    }
    pub fn select_only(&mut self, id: Id) {
        self.primary = Some(id);
        self.extra.clear();
    }
    pub fn clear(&mut self) {
        self.primary = None;
        self.extra.clear();
    }
    pub fn clear_set(&mut self) {
        self.extra.clear();
    }
    pub fn replace_set(&mut self, set: HashSet<Id>, primary: Option<Id>) {
        self.extra = set;
        self.primary = primary;
    }
    pub fn add(&mut self, id: Id) {
        self.extra.insert(id);
    }
    pub fn is_selected(&self, id: &Id) -> bool {
        self.primary.as_ref().is_some_and(|p| p == id) || self.extra.contains(id)
    }
    pub fn all(&self) -> HashSet<Id> {
        let mut s = self.extra.clone();
        if let Some(p) = &self.primary {
            s.insert(p.clone());
        }
        s
    }
    pub fn is_empty(&self) -> bool {
        self.primary.is_none() && self.extra.is_empty()
    }
    pub fn count(&self) -> usize {
        self.extra.len() + usize::from(self.primary.is_some())
    }
}

// ── Context Menu Types ───────────────────────────────────────────────────────

use ratatui::layout::Rect;
use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct MenuItemSpec {
    pub label: &'static str,
    pub shortcut: Option<char>,
    pub color_hint: Option<Color>,
}

impl MenuItemSpec {
    pub const fn new(label: &'static str) -> Self {
        Self {
            label,
            shortcut: None,
            color_hint: None,
        }
    }
    pub const fn shortcut(mut self, c: char) -> Self {
        self.shortcut = Some(c);
        self
    }
    pub const fn color(mut self, c: Color) -> Self {
        self.color_hint = Some(c);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub x: u16,
    pub y: u16,
    pub selected: usize,
    pub items: Vec<MenuItemSpec>,
}

impl ContextMenu {
    pub fn new(x: u16, y: u16, items: Vec<MenuItemSpec>) -> Self {
        Self {
            x,
            y,
            selected: 0,
            items,
        }
    }
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }
    pub fn find_shortcut(&self, ch: char) -> Option<usize> {
        let cl = ch.to_ascii_lowercase();
        self.items
            .iter()
            .position(|i| i.shortcut.is_some_and(|s| s.to_ascii_lowercase() == cl))
    }
    /// On-screen rect for the menu, clamped inside `area`.
    pub fn rect(&self, area: Rect) -> Rect {
        let max_content = self
            .items
            .iter()
            .map(|i| {
                let base = i.label.chars().count();
                let square = if i.color_hint.is_some() { 2 } else { 0 }; // "■ "
                let shortcut = i.shortcut.map_or(0, |_| 2); // "c "
                base + square + shortcut + 4 // 2 left + 2 right pad
            })
            .max()
            .unwrap_or(0);
        let width = max_content.max(8) as u16;
        let height = self.items.len() as u16;
        let x = self
            .x
            .min(area.x.saturating_add(area.width.saturating_sub(width)));
        let y = self
            .y
            .min(area.y.saturating_add(area.height.saturating_sub(height)));
        Rect::new(x, y, width, height)
    }
    /// Menu row index at a screen position, if any.
    pub fn row_at(&self, rect: Rect, col: u16, row: u16) -> Option<usize> {
        if col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
        {
            let idx = (row - rect.y) as usize;
            (idx < self.items.len()).then_some(idx)
        } else {
            None
        }
    }
}

// ── Marquee Selection State ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MarqueeState {
    pub start: Option<(f64, f64)>,
    pub end: Option<(f64, f64)>,
    pub threshold_cells: u32,
}

impl MarqueeState {
    pub fn new(threshold_cells: u32) -> Self {
        Self {
            start: None,
            end: None,
            threshold_cells,
        }
    }
    pub fn on_down(&mut self, x: f64, y: f64) {
        self.start = Some((x, y));
        self.end = Some((x, y));
    }
    pub fn on_drag(&mut self, x: f64, y: f64) {
        self.end = Some((x, y));
    }
    pub fn is_dragging_screen(
        &self,
        sx_now: u16,
        sy_now: u16,
        sx_start: u16,
        sy_start: u16,
    ) -> bool {
        let moved = (sx_now as i32 - sx_start as i32).unsigned_abs()
            + (sy_now as i32 - sy_start as i32).unsigned_abs();
        moved > self.threshold_cells
    }
    pub fn commit_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let s = self.start?;
        let e = self.end?;
        let (min_x, max_x) = (s.0.min(e.0), s.0.max(e.0));
        let (min_y, max_y) = (s.1.min(e.1), s.1.max(e.1));
        Some((min_x, min_y, max_x, max_y))
    }
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
    }
}

// ── Node Specification for Graph Building ─────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NodeSpec {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub folder: String,
    pub links: Vec<String>,
}

// ── Graph Node Data ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GraphNodeData {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub link_count: usize,
    pub folder: String,
}

// ── Menu Specs ───────────────────────────────────────────────────────────────

pub fn menu_specs(extra_multi: bool, node_selected: bool) -> Vec<MenuItemSpec> {
    if extra_multi {
        vec![
            MenuItemSpec::new("Show Group").shortcut('g'),
            MenuItemSpec::new("Delete Node").shortcut('x'),
        ]
    } else if node_selected {
        vec![
            MenuItemSpec::new("Create Connection").shortcut('c'),
            MenuItemSpec::new("Delete Connection").shortcut('d'),
            MenuItemSpec::new("Local Graph").shortcut('l'),
            MenuItemSpec::new("Delete Node").shortcut('x'),
        ]
    } else {
        vec![]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    CreateConnection,
    DeleteConnection,
    LocalGraph,
    ShowGroup,
    DeleteNode,
}

pub fn menu_item_from_label(label: &str) -> Option<MenuItem> {
    match label {
        "Create Connection" => Some(MenuItem::CreateConnection),
        "Delete Connection" => Some(MenuItem::DeleteConnection),
        "Local Graph" => Some(MenuItem::LocalGraph),
        "Show Group" => Some(MenuItem::ShowGroup),
        "Delete Node" => Some(MenuItem::DeleteNode),
        _ => None,
    }
}

// ── Mode Banner ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeBanner {
    CreateConnection,
    DeleteConnection,
    BoxSelect,
    LocalGraph,
    GroupedGraph,
}

// ── Static Layout Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct StaticNode {
    index: NodeIndex,
    degree: usize,
    key: String,
    relative_pos: (f64, f64),
}

#[derive(Debug, Clone)]
struct StaticComponent {
    nodes: Vec<StaticNode>,
    key: String,
    center: (f64, f64),
    disk_radius: f64,
    envelope_radius: f64,
}

// ── Graph State ──────────────────────────────────────────────────────────────

pub struct GraphState {
    pub simulation: Simulation<GraphNodeData, ()>,
    pub viewport: crate::viewport::Viewport,
    pub selection: Selection<NodeIndex>,
    pub dragging_node: Option<NodeIndex>,
    pub drag_target: Option<(f32, f32)>,
    pub is_settled: bool,
    pub alpha: f32,
    pub graph_bounds: (f64, f64, f64, f64),
    pub render_cache: Mutex<crate::render::RenderCache>,
    pub mouse_pos: Option<(u16, u16)>,
    pub physics_worker_active: bool,
    pub physics_ideal_distance: f64,
    pub context_menu: Option<ContextMenu>,
    pub connection_source: Option<NodeIndex>,
    pub deleting_connection_source: Option<NodeIndex>,
    pub marquee: MarqueeState,
    pub right_down_pos: Option<(u16, u16)>,
    pub mode_banner: Option<ModeBanner>,
}

impl GraphState {
    pub fn new(simulation: Simulation<GraphNodeData, ()>, viewport: crate::viewport::Viewport) -> Self {
        Self {
            simulation,
            viewport,
            selection: Selection::new(),
            dragging_node: None,
            drag_target: None,
            is_settled: false,
            alpha: 1.0,
            graph_bounds: (0.0, 0.0, 0.0, 0.0),
            render_cache: Mutex::new(crate::render::RenderCache::default()),
            mouse_pos: None,
            physics_worker_active: false,
            physics_ideal_distance: 80.0,
            context_menu: None,
            connection_source: None,
            deleting_connection_source: None,
            marquee: MarqueeState::new(3),
            right_down_pos: None,
            mode_banner: None,
        }
    }

    /// Build a full graph state from node specs: graph → simulation →
    /// auto-fitted viewport → bounds. Matches clin's original `GraphState::new`.
    pub fn from_specs(nodes: &[NodeSpec], settings: &Settings) -> anyhow::Result<Self> {
        let graph = build_graph(nodes, settings)?;
        let simulation = create_simulation(graph, settings);
        let mut state = Self::new(simulation, crate::viewport::Viewport::default());
        state.alpha = 0.4;
        state.physics_ideal_distance = settings.physics.ideal_distance;
        let padding = if settings.interaction.auto_fit_padding.is_finite()
            && settings.interaction.auto_fit_padding > 0.0
        {
            settings.interaction.auto_fit_padding
        } else {
            1.4
        };
        state.viewport = state
            .viewport
            .auto_fit_from_graph(state.simulation.get_graph(), padding);
        state.graph_bounds = crate::render::compute_graph_bounds(state.simulation.get_graph());
        Ok(state)
    }

    pub fn open_context_menu(&mut self, screen_x: u16, screen_y: u16, _world: (f64, f64)) {
        let specs = menu_specs(
            !self.selection.extra.is_empty(),
            self.selection.primary.is_some(),
        );
        if specs.is_empty() {
            return;
        }
        self.context_menu = Some(ContextMenu::new(screen_x, screen_y, specs));
    }

    pub fn close_menu(&mut self) {
        self.context_menu = None;
    }

    pub fn reheat(&mut self, target: f32) {
        if self.physics_worker_active && target > self.alpha {
            self.alpha = target;
            self.is_settled = false;
        }
    }

    pub fn apply_static_cluster_layout(&mut self, ideal_distance: f64) -> bool {
        let graph = self.simulation.get_graph();
        let node_count = graph.node_count();
        if node_count == 0 {
            return false;
        }

        let mut components = collect_static_components(graph);
        let node_positions = match layout_static_components(&mut components, ideal_distance) {
            Some(pos) => pos,
            None => return false,
        };

        let graph_mut = self.simulation.get_graph_mut();
        for (idx, pos) in node_positions {
            if let Some(node) = graph_mut.node_weight_mut(idx) {
                node.location = pos;
                node.old_location = pos;
                node.velocity = fdg_sim::glam::Vec3::ZERO;
            }
        }

        // Recompute derived state
        self.viewport = self.viewport.auto_fit_from_graph(graph_mut, 1.4);
        self.graph_bounds = crate::render::compute_graph_bounds(graph_mut);

        self.is_settled = true;
        self.alpha = 0.0;
        self.physics_worker_active = false;
        self.render_cache.lock().minimap_dirty = true;

        true
    }
}

// ── Graph Building ───────────────────────────────────────────────────────────

pub fn build_graph(
    nodes: &[NodeSpec],
    settings: &Settings,
) -> anyhow::Result<ForceGraph<GraphNodeData, ()>> {
    let mut graph: ForceGraph<GraphNodeData, ()> = ForceGraph::default();
    let mut title_to_index: HashMap<String, NodeIndex> = HashMap::new();

    let show_orphan = settings.filter.show_orphan;

    // 1. Filter out nodes excluded by tags
    let mut valid_nodes: Vec<&NodeSpec> = Vec::new();
    for node in nodes {
        if !settings.filter.exclude_tags.is_empty()
            && node
                .tags
                .iter()
                .any(|t| settings.filter.exclude_tags.contains(t))
        {
            continue;
        }
        valid_nodes.push(node);
    }

    // 2. Map valid titles for edge validation
    let valid_titles: HashSet<String> = valid_nodes
        .iter()
        .map(|n| n.title.to_lowercase())
        .collect();

    // 3. Find titles that participate in at least one valid edge
    let mut has_valid_edge = HashSet::new();
    if !show_orphan {
        for node in &valid_nodes {
            let source_title = node.title.to_lowercase();
            for link in &node.links {
                let target_title = link.to_lowercase();
                if target_title != source_title && valid_titles.contains(&target_title) {
                    has_valid_edge.insert(source_title.clone());
                    has_valid_edge.insert(target_title);
                }
            }
        }
    }

    // 4. Collect final candidates (excluding orphans if requested)
    let mut candidates: Vec<&NodeSpec> = Vec::new();
    for node in valid_nodes {
        if !show_orphan && !has_valid_edge.contains(&node.title.to_lowercase()) {
            continue;
        }
        candidates.push(node);
    }

    // Apply max_node cap: keep most-connected nodes
    let max_node = settings.max_node;
    if max_node > 0 && candidates.len() > max_node {
        candidates.sort_by_key(|b| std::cmp::Reverse(b.links.len()));
        candidates.truncate(max_node);
    }

    // Insert into force graph
    for node in &candidates {
        let data = GraphNodeData {
            id: node.id.clone(),
            title: node.title.clone(),
            tags: node.tags.clone(),
            link_count: 0, // filled in below from total degree
            folder: node.folder.clone(),
        };

        let idx = graph.add_force_node(&node.title, data);
        title_to_index.insert(node.title.to_lowercase(), idx);
    }

    let mut has_final_edge = std::collections::HashSet::new();

    for node in nodes {
        let source_title = node.title.to_lowercase();

        let source_idx = match title_to_index.get(&source_title) {
            Some(&idx) => idx,
            None => continue,
        };

        let mut seen_targets = std::collections::HashSet::new();
        for link in &node.links {
            let target_lower = link.to_lowercase();
            if let Some(&target_idx) = title_to_index.get(&target_lower)
                && target_idx != source_idx
                && seen_targets.insert(target_idx)
                && graph.edges_connecting(source_idx, target_idx).count() == 0
            {
                graph.add_edge(source_idx, target_idx, ());
                has_final_edge.insert(source_idx);
                has_final_edge.insert(target_idx);
            }
        }
    }

    if !show_orphan {
        let mut to_remove = Vec::new();
        for idx in graph.node_indices() {
            if !has_final_edge.contains(&idx) {
                to_remove.push(idx);
            }
        }
        to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for idx in to_remove {
            graph.remove_node(idx);
        }
    }

    // link_count = total degree (outgoing wikilinks + backlinks), not just the
    // node's outgoing links. Matches GraphState::apply_connection_change.
    let indices: Vec<NodeIndex> = graph.node_indices().collect();
    for idx in indices {
        let degree = graph.edges(idx).count();
        if let Some(n) = graph.node_weight_mut(idx) {
            n.data.link_count = degree;
        }
    }

    Ok(graph)
}

pub fn create_simulation(
    graph: ForceGraph<GraphNodeData, ()>,
    settings: &Settings,
) -> Simulation<GraphNodeData, ()> {
    let force = fdg_sim::force::handy(settings.physics.ideal_distance as f32, settings.physics.damping, true, true);
    let params = SimulationParameters::new(settings.physics.max_iterations as f32, fdg_sim::Dimensions::Two, force);
    Simulation::from_graph(graph, params)
}

// ── Static Layout Implementation ─────────────────────────────────────────────

fn collect_static_components(graph: &ForceGraph<GraphNodeData, ()>) -> Vec<StaticComponent> {
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    let mut start_nodes: Vec<NodeIndex> = graph.node_indices().collect();
    start_nodes.sort_by(|&a, &b| {
        let node_a = &graph[a];
        let node_b = &graph[b];
        node_a
            .data
            .id
            .cmp(&node_b.data.id)
            .then_with(|| a.cmp(&b))
    });

    for &start_node in &start_nodes {
        if visited.contains(&start_node) {
            continue;
        }

        let mut component_nodes = Vec::new();
        let mut queue = std::collections::VecDeque::new();

        visited.insert(start_node);
        queue.push_back(start_node);

        while let Some(curr) = queue.pop_front() {
            component_nodes.push(curr);

            let mut neighbors: Vec<NodeIndex> = graph.neighbors(curr).collect();
            neighbors.sort_by(|&a, &b| {
                let node_a = &graph[a];
                let node_b = &graph[b];
                node_a
                    .data
                    .id
                    .cmp(&node_b.data.id)
                    .then_with(|| a.cmp(&b))
            });

            for nbr in neighbors {
                if !visited.contains(&nbr) {
                    visited.insert(nbr);
                    queue.push_back(nbr);
                }
            }
        }

        // Sort component's nodes deterministically by id and index to identify first_node key
        component_nodes.sort_by(|&a, &b| {
            let node_a = &graph[a];
            let node_b = &graph[b];
            node_a
                .data
                .id
                .cmp(&node_b.data.id)
                .then_with(|| a.cmp(&b))
        });

        if !component_nodes.is_empty() {
            let first_node_idx = component_nodes[0];
            let key = graph[first_node_idx].data.id.clone();

            let mut static_nodes: Vec<StaticNode> = component_nodes
                .into_iter()
                .map(|idx| StaticNode {
                    index: idx,
                    degree: graph.neighbors(idx).count(),
                    key: graph[idx].data.id.clone(),
                    relative_pos: (0.0, 0.0),
                })
                .collect();

            static_nodes.sort_by(|a, b| {
                b.degree
                    .cmp(&a.degree)
                    .then_with(|| a.key.cmp(&b.key))
                    .then_with(|| a.index.cmp(&b.index))
            });

            components.push(StaticComponent {
                nodes: static_nodes,
                key,
                center: (0.0, 0.0),
                disk_radius: 0.0,
                envelope_radius: 0.0,
            });
        }
    }

    // Sort final components by (Reverse(nodes.len()), key)
    components.sort_by(|a, b| {
        let len_cmp = b.nodes.len().cmp(&a.nodes.len());
        if len_cmp != std::cmp::Ordering::Equal {
            len_cmp
        } else {
            a.key.cmp(&b.key)
        }
    });

    components
}

const STATIC_LAYOUT_SLOT_RESERVE: f64 = 1.15;
const MAX_STATIC_LAYOUT_ANGULAR_JITTER: f64 = 0.15;

fn stable_layout_hash(component_key: &str, node_key: &str, ring: usize, stream: u8) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let prime = 0x0100_0000_01b3_u64;

    let mut update_hash = |byte: u8| {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(prime);
    };

    for &b in component_key.as_bytes() {
        update_hash(b);
    }
    update_hash(0xff);
    for &b in node_key.as_bytes() {
        update_hash(b);
    }
    update_hash(0xff);
    for &b in &(ring as u64).to_le_bytes() {
        update_hash(b);
    }
    update_hash(stream);

    hash
}

fn stable_layout_unit(component_key: &str, node_key: &str, ring: usize, stream: u8) -> f64 {
    stable_layout_hash(component_key, node_key, ring, stream) as f64 / u64::MAX as f64
}

fn layout_static_components(
    components: &mut [StaticComponent],
    spacing: f64,
) -> Option<Vec<(NodeIndex, fdg_sim::glam::Vec3)>> {
    let spacing = if spacing.is_finite() && spacing > 0.0 {
        spacing
    } else {
        80.0
    };

    for c in components.iter_mut() {
        let n = c.nodes.len();
        if n == 0 {
            c.disk_radius = 0.0;
            c.envelope_radius = 0.0;
        } else if n == 1 {
            c.nodes[0].relative_pos = (0.0, 0.0);
            c.disk_radius = 0.0;
            c.envelope_radius = spacing;
        } else {
            // Place nodes[0] (highest degree) at center (0, 0)
            c.nodes[0].relative_pos = (0.0, 0.0);

            // Group remaining nodes by degree
            let mut groups = Vec::new();
            {
                let mut current_group = Vec::new();
                let mut current_degree = c.nodes[1].degree;
                current_group.push(1);
                for idx in 2..n {
                    if c.nodes[idx].degree == current_degree {
                        current_group.push(idx);
                    } else {
                        groups.push(current_group);
                        current_group = vec![idx];
                        current_degree = c.nodes[idx].degree;
                    }
                }
                if !current_group.is_empty() {
                    groups.push(current_group);
                }
            }

            // Lay out groups in concentric rings
            let mut r = 1;
            let mut last_used_ring_radius = 0.0;

            for group in groups {
                let mut group_remaining = group.len();
                let mut group_idx = 0;

                while group_remaining > 0 {
                    let ring_radius = spacing * r as f64;
                    last_used_ring_radius = ring_radius;

                    // Calculate slot capacity for this ring
                    let mut slot_capacity = 1;
                    let upper_bound =
                        (2.0 * std::f64::consts::PI * ring_radius / spacing).ceil() as usize + 2;
                    for sc in (1..=upper_bound).rev() {
                        if sc == 1 {
                            slot_capacity = 1;
                            break;
                        }
                        let sin_val = (std::f64::consts::PI / sc as f64).sin();
                        if 2.0 * ring_radius * sin_val >= spacing * STATIC_LAYOUT_SLOT_RESERVE {
                            slot_capacity = sc;
                            break;
                        }
                    }

                    let used_slots = std::cmp::min(group_remaining, slot_capacity);
                    let sector_angle = 2.0 * std::f64::consts::PI / used_slots as f64;
                    let minimum_angle = 2.0 * ((spacing / (2.0 * ring_radius)).min(1.0)).asin();
                    let available_slack = (sector_angle - minimum_angle).max(0.0);
                    let jitter_limit = if used_slots <= 1 {
                        0.0
                    } else {
                        MAX_STATIC_LAYOUT_ANGULAR_JITTER.min(available_slack * 0.45)
                    };
                    let ring_phase =
                        stable_layout_unit(&c.key, "", r, 0) * 2.0 * std::f64::consts::PI;

                    for slot in 0..used_slots {
                        let node_idx = group[group_idx + slot];
                        let node_key = &c.nodes[node_idx].key;
                        let signed_jitter =
                            (stable_layout_unit(&c.key, node_key, r, 1) * 2.0 - 1.0) * jitter_limit;
                        let angle = ring_phase + sector_angle * slot as f64 + signed_jitter;
                        let rx = ring_radius * angle.cos();
                        let ry = ring_radius * angle.sin();
                        c.nodes[node_idx].relative_pos = (rx, ry);
                    }

                    group_remaining -= used_slots;
                    group_idx += used_slots;
                    r += 1;
                }
            }

            c.disk_radius = last_used_ring_radius;
            c.envelope_radius = c.disk_radius + spacing;
        }
    }

    if components.is_empty() {
        return Some(Vec::new());
    }

    let gap = spacing * 4.0;
    components[0].center = (0.0, 0.0);
    let mut occupied_outer_radius = components[0].envelope_radius;

    let mut idx = 1;
    while idx < components.len() {
        let remaining_count = components.len() - idx;
        let next_envelope = components[idx].envelope_radius;
        let ring_radius = occupied_outer_radius + next_envelope + gap;
        let ring_max_envelope = next_envelope;

        let mut slot_count = 1;
        for sc in (2..=remaining_count).rev() {
            let sin_val = (std::f64::consts::PI / sc as f64).sin();
            if 2.0 * ring_radius * sin_val >= 2.0 * ring_max_envelope + gap {
                slot_count = sc;
                break;
            }
        }

        for slot in 0..slot_count {
            let c_idx = idx + slot;
            let angle = 2.0 * std::f64::consts::PI * (slot as f64) / (slot_count as f64);
            let cx = ring_radius * angle.cos();
            let cy = ring_radius * angle.sin();
            components[c_idx].center = (cx, cy);
        }

        occupied_outer_radius = ring_radius + ring_max_envelope;
        idx += slot_count;
    }

    let mut node_positions = Vec::new();
    for c in components.iter() {
        let (cx, cy) = c.center;
        if !cx.is_finite() || !cy.is_finite() {
            return None;
        }

        for node in &c.nodes {
            let nx = cx + node.relative_pos.0;
            let ny = cy + node.relative_pos.1;
            if !nx.is_finite() || !ny.is_finite() {
                return None;
            }
            let pos = fdg_sim::glam::Vec3::new(nx as f32, ny as f32, 0.0);
            node_positions.push((node.index, pos));
        }
    }

    Some(node_positions)
}

// ── Utilities ────────────────────────────────────────────────────────────────

pub(crate) fn nodes_in_rect(
    graph: &ForceGraph<GraphNodeData, ()>,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> impl Iterator<Item = NodeIndex> + '_ {
    graph.node_indices().filter(move |idx| {
        let l = graph[*idx].location;
        (l.x as f64) >= min_x
            && (l.x as f64) <= max_x
            && (l.y as f64) >= min_y
            && (l.y as f64) <= max_y
    })
}

pub fn search_nodes(
    sim: &Simulation<GraphNodeData, ()>,
    query: &str,
    max_results: usize,
) -> Vec<(NodeIndex, String)> {
    let lower = query.to_lowercase();
    let graph = sim.get_graph();
    let mut matches: Vec<(NodeIndex, String)> = graph
        .node_indices()
        .filter_map(|idx| {
            let node = &graph[idx];
            let title = &node.data.title;
            let tags = &node.data.tags;
            
            if title.to_lowercase().contains(&lower) || tags.iter().any(|t| t.to_lowercase().contains(&lower)) {
                Some((idx, title.clone()))
            } else {
                None
            }
        })
        .collect();
    
    matches.truncate(max_results);
    matches
}

// ── Connection Changes ───────────────────────────────────────────────────────

pub fn apply_connection_change(
    sim: &mut Simulation<GraphNodeData, ()>,
    source: NodeIndex,
    target: NodeIndex,
    add: bool,
) {
    let graph = sim.get_graph_mut();
    if add {
        if graph.find_edge(source, target).is_none() {
            graph.add_edge(source, target, ());
            // Update link counts
            if let Some(n) = graph.node_weight_mut(source) {
                n.data.link_count += 1;
            }
            if let Some(n) = graph.node_weight_mut(target) {
                n.data.link_count += 1;
            }
        }
    } else {
        let existing = graph.find_edge(source, target);
        if let Some(e) = existing {
            graph.remove_edge(e);
            // Update link counts
            if let Some(n) = graph.node_weight_mut(source) {
                n.data.link_count = n.data.link_count.saturating_sub(1);
            }
            if let Some(n) = graph.node_weight_mut(target) {
                n.data.link_count = n.data.link_count.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_new() {
        let sel: Selection<usize> = Selection::new();
        assert!(sel.is_empty());
        assert_eq!(sel.count(), 0);
    }

    #[test]
    fn test_selection_select_only() {
        let mut sel: Selection<usize> = Selection::new();
        sel.select_only(5);
        assert_eq!(sel.primary, Some(5));
        assert!(sel.extra.is_empty());
        assert!(sel.is_selected(&5));
        assert!(!sel.is_selected(&10));
    }

    #[test]
    fn test_selection_add() {
        let mut sel: Selection<usize> = Selection::new();
        sel.add(5);
        sel.add(10);
        assert!(sel.primary.is_none());
        assert!(sel.is_selected(&5));
        assert!(sel.is_selected(&10));
        assert_eq!(sel.count(), 2);
    }

    #[test]
    fn test_selection_clear() {
        let mut sel: Selection<usize> = Selection::new();
        sel.select_only(5);
        sel.add(10);
        sel.clear();
        assert!(sel.is_empty());
        assert_eq!(sel.count(), 0);
    }

    #[test]
    fn test_selection_all() {
        let mut sel: Selection<usize> = Selection::new();
        sel.select_only(5);
        sel.add(10);
        let all = sel.all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&5));
        assert!(all.contains(&10));
    }

    #[test]
    fn test_selection_clear_set_keeps_primary() {
        let mut sel: Selection<usize> = Selection::new();
        sel.select_only(5);
        sel.add(10);
        sel.clear_set();
        assert_eq!(sel.primary, Some(5));
        assert!(sel.extra.is_empty());
    }

    #[test]
    fn test_selection_replace_set() {
        let mut sel: Selection<usize> = Selection::new();
        let mut set = std::collections::HashSet::new();
        set.insert(7);
        set.insert(8);
        sel.replace_set(set, Some(7));
        assert_eq!(sel.primary, Some(7));
        assert!(sel.extra.contains(&8));
    }

    #[test]
    fn test_menu_specs() {
        let specs = menu_specs(true, false);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].label, "Show Group");
        assert_eq!(specs[0].shortcut, Some('g'));
        
        let specs = menu_specs(false, true);
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[0].label, "Create Connection");
    }

    #[test]
    fn test_menu_item_from_label() {
        assert!(matches!(menu_item_from_label("Create Connection"), Some(MenuItem::CreateConnection)));
        assert!(matches!(menu_item_from_label("Delete Connection"), Some(MenuItem::DeleteConnection)));
        assert!(menu_item_from_label("Invalid").is_none());
    }

    #[test]
    fn test_marquee_new() {
        let m = MarqueeState::new(3);
        assert_eq!(m.start, None);
        assert_eq!(m.end, None);
    }

    #[test]
    fn test_marquee_on_down() {
        let mut m = MarqueeState::new(3);
        m.on_down(1.0, 2.0);
        assert_eq!(m.start, Some((1.0, 2.0)));
        assert_eq!(m.end, Some((1.0, 2.0)));
    }

    #[test]
    fn test_marquee_on_drag() {
        let mut m = MarqueeState::new(3);
        m.on_down(1.0, 2.0);
        m.on_drag(5.0, 6.0);
        assert_eq!(m.end, Some((5.0, 6.0)));
    }

    #[test]
    fn test_marquee_commit_rect() {
        let mut m = MarqueeState::new(3);
        m.on_down(10.0, 10.0);
        m.on_drag(2.0, 4.0);
        assert_eq!(m.commit_rect(), Some((2.0, 4.0, 10.0, 10.0)));
    }

    #[test]
    fn test_marquee_clear() {
        let mut m = MarqueeState::new(3);
        m.on_down(1.0, 2.0);
        m.clear();
        assert_eq!(m.start, None);
        assert_eq!(m.end, None);
    }
}
