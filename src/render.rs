use std::collections::{HashMap, HashSet};

use fdg_sim::petgraph::graph::NodeIndex;
use fdg_sim::petgraph::visit::{EdgeRef, IntoEdgeReferences};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::canvas::{Canvas, Line, Painter, Shape};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::graph::{ContextMenu, GraphState};
use crate::settings::{
    EdgeColorMode, LabelMode, LegendPosition, NodeColorMode, NodeFill, NodeScale, NodeShape,
    SelectionFocus, Settings,
};
use crate::theme::ThemeColors;
use crate::viewport::{Viewport, node_world_radius};

fn truncate_ellipsis(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return String::new();
    }
    let mut end = end.saturating_sub(1);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return s
            .chars()
            .next()
            .map(|c| format!("{c}…"))
            .unwrap_or_default();
    }
    format!("{}…", &s[..end])
}

const LOCAL_TAG_PALETTE: &[Color] = &[
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Rgb(255, 165, 0),   // Orange
    Color::Rgb(255, 105, 180), // Pink
    Color::Rgb(50, 205, 50),   // Lime
    Color::Rgb(0, 206, 209),   // Turquoise
];

fn tag_color(tag: &str, index: usize, _total: usize, palette: &[Color]) -> Color {
    let palette_len = palette.len();
    if palette_len == 0 {
        return Color::Gray;
    }
    let hash = tag
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    palette[((hash as usize) + index * 7) % palette_len]
}

fn link_count_color(count: usize, max_count: usize, colors: &[Color]) -> Color {
    if max_count == 0 {
        return colors.first().copied().unwrap_or(Color::Gray);
    }
    let idx = (count as f64 / max_count as f64 * colors.len().saturating_sub(1) as f64) as usize;
    colors.get(idx).copied().unwrap_or(Color::Gray)
}

/// Level-of-detail tier determined by visible node count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LodTier {
    /// ≤200 visible nodes: full detail (shapes, colors, tag orbits, labels).
    Full,
    /// 201–1000 visible: shapes + colors, no orbits, selected labels only.
    Medium,
    /// >1000 visible: single-pixel dots, no edges, no labels.
    Minimal,
}

#[derive(Clone)]
pub struct EdgeData {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: Color,
    pub thickness: u16,
}

struct GraphEdgesShape<'a> {
    edges: &'a [EdgeData],
}

impl Shape for GraphEdgesShape<'_> {
    fn draw(&self, painter: &mut Painter) {
        for edge in self.edges {
            if edge.thickness <= 1 {
                Line {
                    x1: edge.x1,
                    y1: edge.y1,
                    x2: edge.x2,
                    y2: edge.y2,
                    color: edge.color,
                }
                .draw(painter);
            } else {
                let dx = edge.x2 - edge.x1;
                let dy = edge.y2 - edge.y1;
                let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                let nx = -dy / len;
                let ny = dx / len;
                let spacing = 0.4;
                for t in 0..edge.thickness {
                    let offset = (t as f64 - (edge.thickness - 1) as f64 / 2.0) * spacing;
                    Line {
                        x1: edge.x1 + nx * offset,
                        y1: edge.y1 + ny * offset,
                        x2: edge.x2 + nx * offset,
                        y2: edge.y2 + ny * offset,
                        color: edge.color,
                    }
                    .draw(painter);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct NodeRenderData {
    pub x: f64,
    pub y: f64,
    pub color: Color,
    pub radius: f64,
    pub extra_tag_colors: Vec<Color>,
    pub is_selected: bool,
    pub is_hovered: bool,
    pub selection_ring_color: Color,
    pub shape: NodeShape,
    /// Paint circles as solid discs instead of outlines.
    pub filled: bool,
    /// Dimmed to gray by selection focus dimming.
    pub dimmed: bool,
    /// Detached outline ring in the node's own color (grow focus feedback).
    pub grow_ring: bool,
}
struct GraphNodesShape<'a> {
    nodes: &'a [NodeRenderData],
    /// World-units step between fill samples; a fraction of one canvas cell so
    /// filled shapes paint solid instead of speckled.
    fill_step: f64,
}

/// Minimum node radius in text rows for a vault of `node_count` notes.
/// `Automatic` eases from the maximum size at ~100 notes down to the classic
/// small look by ~400 notes along a smoothstep curve, so the shrink reads as
/// a gradual zoom instead of a linear ramp with abrupt ends.
pub(crate) fn node_floor_rows(scale: NodeScale, node_count: usize) -> f64 {
    const LARGE_ROWS: f64 = 0.9;
    match scale {
        NodeScale::Fixed(k) => LARGE_ROWS * (k.min(10).saturating_sub(1)) as f64 / 9.0,
        NodeScale::Automatic => {
            let t = ((400.0 - node_count as f64) / 300.0).clamp(0.0, 1.0);
            LARGE_ROWS * t * t * (3.0 - 2.0 * t)
        }
    }
}
/// Fill-sample step in world units: a quarter of the smaller canvas cell
/// dimension, below the braille dot pitch so discs paint solid.
pub(crate) fn fill_step_for(cell_w: f64, cell_h: f64) -> f64 {
    cell_w.min(cell_h) / 4.0
}

/// Paints every canvas point inside `radius` for `shape`, sampling on a
/// `step`-spaced grid; `color_for` picks the color per sample.
fn paint_shape(
    painter: &mut Painter,
    cx: f64,
    cy: f64,
    radius: f64,
    shape: NodeShape,
    step: f64,
    color_for: impl Fn(f64, f64) -> Color,
) {
    let step = step.max(1e-6);
    if step > radius {
        if let Some((px, py)) = painter.get_point(cx, cy) {
            painter.paint(px, py, color_for(0.0, 0.0));
        }
        return;
    }
    let inside = |dx: f64, dy: f64| match shape {
        NodeShape::Circle => dx * dx + dy * dy <= radius * radius,
        NodeShape::Square => dx.abs() <= radius && dy.abs() <= radius,
        NodeShape::Diamond => dx.abs() + dy.abs() <= radius,
    };
    let mut dy = -radius;
    while dy <= radius {
        let mut dx = -radius;
        while dx <= radius {
            if inside(dx, dy) {
                let color = color_for(dx, dy);
                if let Some((px, py)) = painter.get_point(cx + dx, cy + dy) {
                    painter.paint(px, py, color);
                }
            }
            dx += step;
        }
        dy += step;
    }
}

fn draw_outlined_shape(
    painter: &mut Painter,
    cx: f64,
    cy: f64,
    radius: f64,
    shape: NodeShape,
    color: Color,
) {
    match shape {
        NodeShape::Circle => {
            let steps = 16u32;
            for i in 0..steps {
                let a1 = (i as f64) * std::f64::consts::TAU / (steps as f64);
                let a2 = ((i + 1) as f64) * std::f64::consts::TAU / (steps as f64);
                Line {
                    x1: cx + radius * a1.cos(),
                    y1: cy + radius * a1.sin(),
                    x2: cx + radius * a2.cos(),
                    y2: cy + radius * a2.sin(),
                    color,
                }
                .draw(painter);
            }
        }
        NodeShape::Square => {
            Line {
                x1: cx - radius,
                y1: cy - radius,
                x2: cx + radius,
                y2: cy - radius,
                color,
            }
            .draw(painter);
            Line {
                x1: cx + radius,
                y1: cy - radius,
                x2: cx + radius,
                y2: cy + radius,
                color,
            }
            .draw(painter);
            Line {
                x1: cx + radius,
                y1: cy + radius,
                x2: cx - radius,
                y2: cy + radius,
                color,
            }
            .draw(painter);
            Line {
                x1: cx - radius,
                y1: cy + radius,
                x2: cx - radius,
                y2: cy - radius,
                color,
            }
            .draw(painter);
        }
        NodeShape::Diamond => {
            Line {
                x1: cx,
                y1: cy - radius,
                x2: cx + radius,
                y2: cy,
                color,
            }
            .draw(painter);
            Line {
                x1: cx + radius,
                y1: cy,
                x2: cx,
                y2: cy + radius,
                color,
            }
            .draw(painter);
            Line {
                x1: cx,
                y1: cy + radius,
                x2: cx - radius,
                y2: cy,
                color,
            }
            .draw(painter);
            Line {
                x1: cx - radius,
                y1: cy,
                x2: cx,
                y2: cy - radius,
                color,
            }
            .draw(painter);
        }
    }
}

impl Shape for GraphNodesShape<'_> {
    fn draw(&self, painter: &mut Painter) {
        for node in self.nodes {
            let mut current_radius = if node.filled && !node.extra_tag_colors.is_empty() {
                node.radius * 2.25
            } else {
                node.radius
            };

            if node.filled {
                paint_shape(
                    painter,
                    node.x,
                    node.y,
                    node.radius,
                    node.shape,
                    self.fill_step,
                    |_, _| node.color,
                );
            } else {
                draw_outlined_shape(painter, node.x, node.y, node.radius, node.shape, node.color);
            }

            if node.filled && !node.extra_tag_colors.is_empty() {
                let n = node.extra_tag_colors.len();
                for (i, &color) in node.extra_tag_colors.iter().enumerate() {
                    let sat_color = if node.dimmed { Color::DarkGray } else { color };
                    let theta = (i as f64) * std::f64::consts::TAU / (n as f64);
                    let orbit_radius = node.radius * 1.75;
                    let sx = node.x + orbit_radius * theta.cos();
                    let sy = node.y + orbit_radius * theta.sin();

                    paint_shape(
                        painter,
                        sx,
                        sy,
                        node.radius * 0.4,
                        node.shape,
                        self.fill_step,
                        |_, _| sat_color,
                    );
                }
            }

            if node.is_hovered && !node.is_selected {
                current_radius += node.radius * 0.5;
                draw_outlined_shape(
                    painter,
                    node.x,
                    node.y,
                    current_radius,
                    node.shape,
                    Color::White,
                );
            }

            if node.grow_ring {
                current_radius += node.radius * 0.5;
                draw_outlined_shape(
                    painter,
                    node.x,
                    node.y,
                    current_radius,
                    node.shape,
                    node.color,
                );
            }

            if node.is_selected {
                current_radius += 1.5;
                draw_outlined_shape(
                    painter,
                    node.x,
                    node.y,
                    current_radius,
                    node.shape,
                    node.selection_ring_color,
                );
            }
        }
    }
}

#[derive(Clone)]
pub struct LabelData {
    pub node_idx: NodeIndex,
    pub x: f64,
    pub y: f64,
    /// Node centre, so the label can be flipped below the node on collision.
    pub node_y: f64,
}

pub struct FeatureFlags {
    pub show_legend: bool,
    pub grid: bool,
    pub show_minimap: bool,
    pub show_status_bar: bool,
}

pub struct RenderCache {
    pub tag_colors: HashMap<String, Color>,
    pub folder_colors: HashMap<String, Color>,
    pub node_own_color: HashMap<NodeIndex, Color>,
    pub legend_data: Option<Vec<(String, Color)>>,
    pub max_link_count: usize,

    pub edges: Vec<EdgeData>,
    pub nodes: Vec<NodeRenderData>,
    pub labels: Vec<LabelData>,

    pub minimap_grid: Vec<Option<Color>>,

    pub topology_dirty: bool,
    pub minimap_dirty: bool,

    pub visible_nodes: HashSet<NodeIndex>,
    pub selected_neighbors: HashSet<NodeIndex>,
    /// Nodes outside the selected node's neighborhood (drawn muted).
    pub dimmed: HashSet<NodeIndex>,
    pub label_texts: HashMap<NodeIndex, String>,
    pub cached_label_max_length: usize,
}

impl Default for RenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            tag_colors: HashMap::new(),
            folder_colors: HashMap::new(),
            node_own_color: HashMap::new(),
            legend_data: None,
            max_link_count: 0,
            edges: Vec::new(),
            nodes: Vec::new(),
            labels: Vec::new(),
            minimap_grid: Vec::new(),
            topology_dirty: true,
            minimap_dirty: true,
            visible_nodes: HashSet::new(),
            selected_neighbors: HashSet::new(),
            dimmed: HashSet::new(),
            label_texts: HashMap::new(),
            cached_label_max_length: usize::MAX,
        }
    }

    pub fn rebuild_topology(
        &mut self,
        graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
        settings: &Settings,
        colors: &ThemeColors,
        show_legend: bool,
    ) {
        self.max_link_count = graph
            .node_weights()
            .map(|n| n.data.link_count)
            .max()
            .unwrap_or(0);

        self.tag_colors.clear();
        {
            let mut unique_tags: HashSet<String> = HashSet::new();
            for node in graph.node_weights() {
                for tag in &node.data.tags {
                    unique_tags.insert(tag.clone());
                }
            }
            let mut sorted_tags: Vec<String> = unique_tags.into_iter().collect();
            sorted_tags.sort();
            let total = sorted_tags.len().max(1);
            for (i, tag) in sorted_tags.into_iter().enumerate() {
                let c = tag_color(&tag, i, total, &colors.node_colors);
                self.tag_colors.insert(tag, c);
            }
        }

        self.folder_colors.clear();
        {
            let mut unique_folders: HashSet<String> = HashSet::new();
            for node in graph.node_weights() {
                unique_folders.insert(node.data.folder.clone());
            }
            let mut sorted_folders: Vec<String> = unique_folders.into_iter().collect();
            sorted_folders.sort();
            let total = sorted_folders.len().max(1);
            for (i, f) in sorted_folders.into_iter().enumerate() {
                let c = tag_color(&f, i, total, &colors.node_colors);
                self.folder_colors.insert(f, c);
            }
        }

        self.node_own_color.clear();
        for idx in graph.node_indices() {
            let node = &graph[idx];
            let color = match settings.visual.node_color_mode {
                NodeColorMode::Tag => {
                    if let Some(tag) = node.data.tags.first() {
                        self.tag_colors.get(tag).copied().unwrap_or(Color::Gray)
                    } else {
                        Color::Gray
                    }
                }
                NodeColorMode::Folder => self
                    .folder_colors
                    .get(&node.data.folder)
                    .copied()
                    .unwrap_or(Color::Gray),
                NodeColorMode::LinkCount => link_count_color(
                    node.data.link_count,
                    self.max_link_count,
                    &colors.node_colors,
                ),
                NodeColorMode::Uniform => {
                    colors.node_colors.first().copied().unwrap_or(Color::Gray)
                }
            };
            self.node_own_color.insert(idx, color);
        }

        self.legend_data = if show_legend {
            let items = match settings.visual.node_color_mode {
                NodeColorMode::Folder => &self.folder_colors,
                _ => &self.tag_colors,
            };
            if items.len() <= 1 {
                None
            } else {
                let mut sorted: Vec<_> = items.iter().collect();
                sorted.sort_by_key(|(t, _)| t.as_str());
                sorted.truncate(10);
                Some(sorted.into_iter().map(|(t, c)| (t.clone(), *c)).collect())
            }
        } else {
            None
        };

        self.topology_dirty = false;
        self.label_texts.clear();
        for idx in graph.node_indices() {
            let node = &graph[idx];
            let truncated = truncate_ellipsis(&node.data.title, settings.visual.label_max_length);
            self.label_texts.insert(idx, truncated);
        }
        self.cached_label_max_length = settings.visual.label_max_length;
    }

    pub fn fill_edges(
        &mut self,
        graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
        settings: &Settings,
        edge_color: Color,
        tier: LodTier,
        lit: &HashSet<NodeIndex>,
    ) {
        self.edges.clear();

        if tier == LodTier::Minimal {
            // No edges at minimal LOD
            return;
        }

        let uniform_edges = tier == LodTier::Medium;
        for edge in graph.edge_references() {
            let src = &graph[edge.source()];
            let tgt = &graph[edge.target()];
            let dimming = matches!(
                settings.visual.selection_focus,
                SelectionFocus::Dim | SelectionFocus::GrowDim
            );
            let color = if dimming
                && !lit.is_empty()
                && !(lit.contains(&edge.source()) && lit.contains(&edge.target()))
            {
                Color::DarkGray
            } else if uniform_edges {
                edge_color
            } else {
                match settings.visual.edge_color_mode {
                    EdgeColorMode::Source => *self
                        .node_own_color
                        .get(&edge.source())
                        .unwrap_or(&edge_color),
                    EdgeColorMode::Target => *self
                        .node_own_color
                        .get(&edge.target())
                        .unwrap_or(&edge_color),
                    EdgeColorMode::Uniform => edge_color,
                }
            };
            self.edges.push(EdgeData {
                x1: src.location.x as f64,
                y1: src.location.y as f64,
                x2: tgt.location.x as f64,
                y2: tgt.location.y as f64,
                color,
                thickness: if uniform_edges {
                    1
                } else {
                    settings.visual.edge_thickness
                },
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_nodes(
        &mut self,
        graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
        settings: &Settings,
        selected_node: Option<NodeIndex>,
        selected_nodes: &HashSet<NodeIndex>,
        lit: &HashSet<NodeIndex>,
        selection_ring_color: Color,
        hovered_node: Option<NodeIndex>,
        x_bounds: [f64; 2],
        y_bounds: [f64; 2],
        world_per_row: f64,
    ) -> LodTier {
        self.nodes.clear();
        self.visible_nodes.clear();

        for idx in
            crate::graph::nodes_in_rect(graph, x_bounds[0], y_bounds[0], x_bounds[1], y_bounds[1])
        {
            self.visible_nodes.insert(idx);
        }

        // Always include selected node(s) even if off-screen
        if let Some(sel) = selected_node {
            self.visible_nodes.insert(sel);
        }
        for idx in selected_nodes {
            self.visible_nodes.insert(*idx);
        }

        // Determine LOD tier from visible node count.
        let tier = match self.visible_nodes.len() {
            0..=200 => LodTier::Full,
            201..=1000 => LodTier::Medium,
            _ => LodTier::Minimal,
        };

        let focus = settings.visual.selection_focus;
        // `lit`: the selection plus its one-hop neighborhood, passed in by the
        // caller; grow keeps these nodes large, dimming grays everything else.
        self.dimmed.clear();
        if selected_node.is_some() && matches!(focus, SelectionFocus::Dim | SelectionFocus::GrowDim)
        {
            self.dimmed.extend(
                self.visible_nodes
                    .iter()
                    .copied()
                    .filter(|i| !lit.contains(i)),
            );
        }
        let grow = selected_node.is_some()
            && matches!(focus, SelectionFocus::Grow | SelectionFocus::GrowDim);
        let floor_rows = node_floor_rows(settings.visual.node_scale, graph.node_count());
        let floor = floor_rows * world_per_row;

        for &idx in &self.visible_nodes {
            let node = &graph[idx];
            let primary_color = if self.dimmed.contains(&idx) {
                Color::DarkGray
            } else {
                self.node_own_color
                    .get(&idx)
                    .copied()
                    .unwrap_or(Color::Gray)
            };
            let base_radius =
                node_world_radius(settings, self.max_link_count, node.data.link_count);
            let grown = grow && lit.contains(&idx);
            // Grown nodes are slightly enlarged; the detached ring makes them pop.
            let radius = if grown {
                base_radius.max(floor) * 1.2
            } else {
                base_radius.max(floor)
            };
            // Filled per `node_fill`: never, always (but not 1-dot minimal
            // LOD), or dynamically at full detail with size to show for it.
            let filled = match settings.visual.node_fill {
                NodeFill::None => false,
                NodeFill::Filled => true,
                NodeFill::Dynamic => tier == LodTier::Full && (grown || floor_rows > 0.0),
            };

            let is_selected = selected_node == Some(idx) || selected_nodes.contains(&idx);
            let is_hovered = hovered_node == Some(idx) && !is_selected;

            match tier {
                LodTier::Full => {
                    let extra_tag_colors: Vec<Color> = if node.data.tags.is_empty() {
                        Vec::new()
                    } else {
                        let mut colors = Vec::new();
                        let mut pal_idx = 0;
                        for _ in &node.data.tags {
                            while pal_idx < LOCAL_TAG_PALETTE.len()
                                && LOCAL_TAG_PALETTE[pal_idx] == primary_color
                            {
                                pal_idx += 1;
                            }
                            if pal_idx < LOCAL_TAG_PALETTE.len() {
                                colors.push(LOCAL_TAG_PALETTE[pal_idx]);
                                pal_idx += 1;
                            } else {
                                colors.push(Color::White);
                            }
                        }
                        colors
                    };
                    self.nodes.push(NodeRenderData {
                        x: node.location.x as f64,
                        y: node.location.y as f64,
                        color: primary_color,
                        radius,
                        extra_tag_colors,
                        is_selected,
                        is_hovered,
                        selection_ring_color,
                        shape: settings.visual.node_shape,
                        filled,
                        dimmed: self.dimmed.contains(&idx),
                        grow_ring: grown,
                    });
                }
                LodTier::Medium => {
                    // No tag orbits, only selected node gets hover ring
                    self.nodes.push(NodeRenderData {
                        x: node.location.x as f64,
                        y: node.location.y as f64,
                        color: primary_color,
                        radius,
                        extra_tag_colors: Vec::new(),
                        is_selected,
                        is_hovered: false,
                        selection_ring_color,
                        filled,
                        shape: settings.visual.node_shape,
                        dimmed: self.dimmed.contains(&idx),
                        grow_ring: grown,
                    });
                }
                LodTier::Minimal => {
                    // Single-pixel dots, forced Circle shape
                    self.nodes.push(NodeRenderData {
                        x: node.location.x as f64,
                        y: node.location.y as f64,
                        color: primary_color,
                        radius: 1.0,
                        extra_tag_colors: Vec::new(),
                        is_selected,
                        is_hovered: false,
                        selection_ring_color,
                        shape: NodeShape::Circle,
                        filled,
                        dimmed: self.dimmed.contains(&idx),
                        grow_ring: grown,
                    });
                }
            }
        }

        tier
    }
    pub fn fill_labels(
        &mut self,
        graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
        settings: &Settings,
        selected_node: Option<NodeIndex>,
        selected_nodes: &HashSet<NodeIndex>,
        min_offset_y: f64,
        tier: LodTier,
    ) {
        self.labels.clear();

        if self.cached_label_max_length != settings.visual.label_max_length {
            self.label_texts.clear();
            for idx in graph.node_indices() {
                let node = &graph[idx];
                let truncated =
                    truncate_ellipsis(&node.data.title, settings.visual.label_max_length);
                self.label_texts.insert(idx, truncated);
            }
            self.cached_label_max_length = settings.visual.label_max_length;
        }

        match tier {
            LodTier::Minimal => return,
            LodTier::Medium => {
                if let Some(sel) = selected_node
                    && self.visible_nodes.contains(&sel)
                {
                    let node = &graph[sel];
                    let radius = self.nodes.get(sel.index()).map(|n| n.radius).unwrap_or(2.0);
                    self.labels.push(LabelData {
                        node_idx: sel,
                        x: node.location.x as f64,
                        y: node.location.y as f64
                            + radius
                            + settings.visual.label_offset.max(min_offset_y),
                        node_y: node.location.y as f64,
                    });
                }
                return;
            }
            LodTier::Full => {}
        }

        self.selected_neighbors.clear();
        if let Some(sel) = selected_node
            && settings.visual.label_mode == LabelMode::Neighbors
        {
            for edge in graph.edges(sel) {
                if edge.target() != sel {
                    self.selected_neighbors.insert(edge.target());
                }
                if edge.source() != sel {
                    self.selected_neighbors.insert(edge.source());
                }
            }
        }

        let should_show = |idx: NodeIndex| -> bool {
            match settings.visual.label_mode {
                LabelMode::Selected => selected_node == Some(idx) || selected_nodes.contains(&idx),
                LabelMode::Neighbors => {
                    selected_node == Some(idx)
                        || selected_nodes.contains(&idx)
                        || self.selected_neighbors.contains(&idx)
                }
                LabelMode::All => true,
                LabelMode::None => false,
            }
        };

        for &idx in &self.visible_nodes {
            if !should_show(idx) {
                continue;
            }
            let node = &graph[idx];
            let radius = self.nodes.get(idx.index()).map(|n| n.radius).unwrap_or(2.0);
            self.labels.push(LabelData {
                node_idx: idx,
                x: node.location.x as f64,
                y: node.location.y as f64 + radius + settings.visual.label_offset.max(min_offset_y),
                node_y: node.location.y as f64,
            });
        }
        // Stable order so collision resolution does not flicker between frames.
        self.labels.sort_by_key(|l| l.node_idx);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_graph_view(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &GraphState,
    settings: &Settings,
    theme: &ThemeColors,
    flags: &FeatureFlags,
) {
    let canvas_area = canvas_area(area, flags.show_status_bar);
    let aspect = canvas_area.width as f64 / canvas_area.height as f64;
    let viewport = &state.viewport;
    let colors = theme;
    let graph = state.simulation.get_graph();

    let mut cache = state.render_cache.lock();

    if cache.topology_dirty || (flags.show_legend && cache.legend_data.is_none()) {
        cache.rebuild_topology(graph, settings, colors, flags.show_legend);
    }

    // Compute hovered node from mouse position
    let hovered_node = state.mouse_pos.and_then(|(col, row)| {
        let (wx, wy) = viewport.screen_to_world(col, row, canvas_area);
        viewport.hit_test(wx, wy, state, settings, canvas_area, cache.max_link_count)
    });

    let x_bounds = viewport.x_bounds(aspect);
    let y_bounds = viewport.y_bounds(aspect);

    let selected_set: HashSet<NodeIndex> = {
        let mut s = state.selection.extra.clone();
        if let Some(idx) = state.selection.primary {
            s.insert(idx);
        }
        s
    };
    let cell_world_height =
        (y_bounds[1] - y_bounds[0]).abs() / (canvas_area.height as f64).max(1.0);
    let cell_world_width = (x_bounds[1] - x_bounds[0]) / (canvas_area.width as f64).max(1.0);
    let fill_step = fill_step_for(cell_world_width, cell_world_height);
    let lit: HashSet<NodeIndex> = {
        let mut lit = selected_set.clone();
        if let Some(sel) = state.selection.primary {
            for edge in graph.edges(sel) {
                lit.insert(edge.source());
                lit.insert(edge.target());
            }
        }
        lit
    };
    let tier = cache.fill_nodes(
        graph,
        settings,
        state.selection.primary,
        &selected_set,
        &lit,
        colors.selected_indicator_color,
        hovered_node,
        x_bounds,
        y_bounds,
        cell_world_height,
    );
    cache.fill_edges(graph, settings, colors.edge_color, tier, &lit);
    cache.fill_labels(
        graph,
        settings,
        state.selection.primary,
        &selected_set,
        cell_world_height * 1.5,
        tier,
    );
    let edges_ref = &cache.edges;
    let nodes_ref = &cache.nodes;
    let labels_ref = &cache.labels;
    let label_texts_ref = &cache.label_texts;
    let label_colors_ref = &cache.node_own_color;
    let dimmed_ref = &cache.dimmed;

    let block = ratatui::widgets::Block::default().style(
        ratatui::style::Style::default().bg(colors.background_color.unwrap_or(Color::Reset)),
    );

    let cols_per_world_x =
        (canvas_area.width.saturating_sub(1) as f64) / (x_bounds[1] - x_bounds[0]);
    let rows_per_world_y =
        -(canvas_area.height.saturating_sub(1) as f64) / (y_bounds[1] - y_bounds[0]);

    // Resolve label collisions in cell space: above the node first, below it as a
    // fallback, dropped when both would overprint an already placed label.
    let rows_per_world_abs = rows_per_world_y.abs();
    let mut occupied: Vec<(i64, i64, i64)> = Vec::new();
    let mut label_draws: Vec<(f64, f64, ratatui::text::Span<'static>)> = Vec::new();
    for label in labels_ref {
        let Some(text) = label_texts_ref.get(&label.node_idx) else {
            continue;
        };
        let width = text.chars().count() as i64;
        let half_width = width as f64 / 2.0 / cols_per_world_x.max(1e-9);
        let x = label.x - half_width;
        let col0 = ((x - x_bounds[0]) * cols_per_world_x).floor() as i64;
        let below_y = 2.0 * label.node_y - label.y;
        let slot = [label.y, below_y].into_iter().find_map(|y| {
            let row = ((y_bounds[1] - y) * rows_per_world_abs).floor() as i64;
            let free = occupied
                .iter()
                .all(|&(r, c0, c1)| r != row || col0 > c1 || col0 + width < c0);
            free.then_some((row, y))
        });
        let Some((row, y)) = slot else {
            continue;
        };
        occupied.push((row, col0, col0 + width));
        let fg = if dimmed_ref.contains(&label.node_idx) {
            Color::DarkGray
        } else {
            label_colors_ref
                .get(&label.node_idx)
                .copied()
                .unwrap_or(colors.label_color)
        };
        label_draws.push((
            x,
            y,
            ratatui::text::Span::styled(text.clone(), ratatui::style::Style::default().fg(fg)),
        ));
    }

    let canvas = Canvas::default()
        .background_color(colors.background_color.unwrap_or(Color::Reset))
        .x_bounds(x_bounds)
        .y_bounds(y_bounds)
        .block(block)
        .marker(ratatui::symbols::Marker::from(
            settings.visual.canvas_marker,
        ))
        .paint(move |ctx| {
            ctx.draw(&GraphEdgesShape { edges: edges_ref });
            ctx.layer();
            ctx.draw(&GraphNodesShape {
                nodes: nodes_ref,
                fill_step,
            });
            for (x, y, span) in &label_draws {
                ctx.print(*x, *y, span.clone());
            }
        });

    frame.render_widget(canvas, canvas_area);
    draw_canvas_grid(
        frame,
        canvas_area,
        flags.grid,
        GridProjection {
            world_left: x_bounds[0],
            world_right: x_bounds[1],
            world_top: y_bounds[0],
            world_bottom: y_bounds[1],
            origin_col: canvas_area.left() as f64 - x_bounds[0] * cols_per_world_x,
            origin_row: canvas_area.top() as f64 - y_bounds[1] * rows_per_world_y,
            cols_per_world_x,
            rows_per_world_y,
        },
        colors.grid_color,
        state.viewport.zoom,
    );

    if flags.show_legend
        && let Some(ref items) = cache.legend_data
    {
        let max_len = items
            .iter()
            .map(|(t, _): &(String, ratatui::style::Color)| t.len())
            .max()
            .unwrap_or(0);
        let legend_width = (max_len + 4) as u16;
        let legend_height = (items.len() as u16).min(10) + 2;
        let (legend_x, legend_y) = match LegendPosition::BottomRight {
            LegendPosition::TopLeft => (canvas_area.x, canvas_area.y),
            LegendPosition::TopRight => (
                canvas_area.x + canvas_area.width.saturating_sub(legend_width),
                canvas_area.y,
            ),
            LegendPosition::BottomLeft => (
                canvas_area.x,
                canvas_area.y + canvas_area.height.saturating_sub(legend_height + 1),
            ),
            LegendPosition::BottomRight => (
                canvas_area.x + canvas_area.width.saturating_sub(legend_width),
                canvas_area.y + canvas_area.height.saturating_sub(legend_height + 1),
            ),
        };
        let legend_area =
            ratatui::layout::Rect::new(legend_x, legend_y, legend_width, legend_height);
        let legend_text: Vec<ratatui::text::Line> = items
            .iter()
            .map(|(t, c): &(String, ratatui::style::Color)| {
                let display_text = if t.is_empty() { "/" } else { t };
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("● ", ratatui::style::Style::default().fg(*c)),
                    ratatui::text::Span::styled(
                        display_text,
                        ratatui::style::Style::default().fg(colors.label_color),
                    ),
                ])
            })
            .collect();
        let legend_widget = ratatui::widgets::Paragraph::new(legend_text).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(ratatui::style::Style::default().fg(colors.border_color))
                .style(
                    ratatui::style::Style::default()
                        .bg(colors.background_color.unwrap_or(Color::Black)),
                ),
        );
        frame.render_widget(Clear, legend_area);
        frame.render_widget(legend_widget, legend_area);
    }

    if flags.show_minimap {
        let minimap_area = compute_minimap_area(canvas_area, settings);

        // Mark dirty when physics is active (positions may change)
        cache.minimap_dirty = cache.minimap_dirty || !state.is_settled;

        let mut minimap_grid = std::mem::take(&mut cache.minimap_grid);
        draw_minimap(
            frame,
            minimap_area,
            MinimapParams {
                viewport,
                graph,
                graph_bounds: state.graph_bounds,
                node_colors: &cache.node_own_color,
                colors,
            },
            &mut minimap_grid,
            cache.minimap_dirty,
        );

        cache.minimap_grid = minimap_grid;
        cache.minimap_dirty = false;
    }

    // Right-drag box-select rectangle.
    if let (Some(start), Some(curr)) = (state.marquee.start, state.marquee.end)
        && state.right_down_pos.is_some()
    {
        let (col0, row0) = viewport.world_to_screen(start.0, start.1, canvas_area);
        let (col1, row1) = viewport.world_to_screen(curr.0, curr.1, canvas_area);
        let min_col = col0.min(col1).floor().max(canvas_area.x as f64) as u16;
        let max_col = col0
            .max(col1)
            .ceil()
            .min((canvas_area.x + canvas_area.width.saturating_sub(1)) as f64)
            as u16;
        let min_row = row0.min(row1).floor().max(canvas_area.y as f64) as u16;
        let max_row = row0
            .max(row1)
            .ceil()
            .min((canvas_area.y + canvas_area.height.saturating_sub(1)) as f64)
            as u16;

        let screen_rect = ratatui::layout::Rect::new(
            min_col,
            min_row,
            max_col.saturating_sub(min_col).saturating_add(1),
            max_row.saturating_sub(min_row).saturating_add(1),
        );
        let fill = muted_fill(theme.selected_indicator_color);
        draw_rect_filled(frame, screen_rect, fill);
    }

    if settings.visual.show_looking_glass && state.selection.primary.is_some() {
        draw_looking_glass(frame, canvas_area, state, settings, colors, &cache);
    }

    if let Some(menu) = &state.context_menu {
        render_context_menu(frame, canvas_area, menu, theme, state.mouse_pos);
    }
}

/// Rect passed to Canvas drawing and geometry: `area` with the bottom status-bar
/// row removed when it is shown. Render and input MUST use this same rect so
/// hover and click map mouse→world identically.
pub fn canvas_area(area: Rect, show_status_bar: bool) -> Rect {
    let mut c = area;
    if show_status_bar {
        c.height = c.height.saturating_sub(1);
    }
    c
}

pub fn compute_minimap_area(frame_area: Rect, settings: &Settings) -> Rect {
    let w = settings.visual.minimap_width;
    let h = settings.visual.minimap_height;
    let (x, y) = match settings.visual.minimap_position {
        LegendPosition::TopLeft => (frame_area.x, frame_area.y),
        LegendPosition::TopRight => (
            frame_area.x + frame_area.width.saturating_sub(w),
            frame_area.y,
        ),
        LegendPosition::BottomLeft => (
            frame_area.x,
            frame_area.y + frame_area.height.saturating_sub(h),
        ),
        LegendPosition::BottomRight => (
            frame_area.x + frame_area.width.saturating_sub(w + 1),
            frame_area.y + frame_area.height.saturating_sub(h + 1),
        ),
    };
    Rect::new(x, y, w, h)
}

pub fn compute_graph_bounds(
    graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for node in graph.node_weights() {
        let x = node.location.x as f64;
        let y = node.location.y as f64;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }

    if min_x == f64::MAX {
        min_x = -100.0;
        max_x = 100.0;
        min_y = -100.0;
        max_y = 100.0;
    }

    let pad_x = (max_x - min_x) * 0.1 + 1.0;
    let pad_y = (max_y - min_y) * 0.1 + 1.0;
    (min_x - pad_x, max_x + pad_x, min_y - pad_y, max_y + pad_y)
}

struct MinimapParams<'a> {
    viewport: &'a Viewport,
    graph: &'a fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
    graph_bounds: (f64, f64, f64, f64),
    node_colors: &'a HashMap<NodeIndex, Color>,
    colors: &'a ThemeColors,
}
fn draw_minimap(
    frame: &mut ratatui::Frame,
    area: Rect,
    params: MinimapParams<'_>,
    grid: &mut Vec<Option<Color>>,
    dirty: bool,
) {
    let (wx_min, wx_max, wy_min, wy_max) = params.graph_bounds;
    let aspect = area.width as f64 / area.height as f64;
    let vp_x = params.viewport.x_bounds(aspect);
    let vp_y = params.viewport.y_bounds(aspect);

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(params.colors.minimap_border_color))
        .style(
            ratatui::style::Style::default()
                .bg(params.colors.minimap_bg_color.unwrap_or(Color::Black)),
        );
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let iw = inner.width as usize;
    let ih = inner.height as usize;
    let sub_h = ih * 2;

    let world_w = wx_max - wx_min;
    let world_h = wy_max - wy_min;

    if world_w <= 0.0 || world_h <= 0.0 {
        return;
    }

    let world_to_col = |x: f64| -> usize {
        let t = (x - wx_min) / world_w;
        let col = (t * iw as f64).floor() as isize;
        col.clamp(0, (iw as isize) - 1) as usize
    };

    let world_to_subrow = |y: f64| -> usize {
        let t = (wy_max - y) / world_h;
        let row = (t * sub_h as f64).floor() as isize;
        row.clamp(0, (sub_h as isize) - 1) as usize
    };

    let world_to_row = |y: f64| -> usize {
        let t = (wy_max - y) / world_h;
        let row = (t * ih as f64).floor() as isize;
        row.clamp(0, (ih as isize) - 1) as usize
    };

    // Only rebuild the pixel grid when dirty (physics changed positions)
    let grid_size = sub_h * iw;
    if dirty || grid.len() != grid_size {
        grid.resize(grid_size, None);
        grid.fill(None);
        for idx in params.graph.node_indices() {
            let node = &params.graph[idx];
            let nx = node.location.x as f64;
            let ny = node.location.y as f64;
            let col = world_to_col(nx);
            let sub_row = world_to_subrow(ny);
            let color = params.node_colors.get(&idx).copied().unwrap_or(Color::Gray);
            grid[sub_row * iw + col] = Some(color);
        }
    }

    let buf = frame.buffer_mut();
    let bg_color: Option<Color> = params.colors.minimap_bg_color;

    for cell_row in 0..ih {
        let top_sub = cell_row * 2;
        let bot_sub = cell_row * 2 + 1;
        for col in 0..iw {
            let top_color = grid[top_sub * iw + col];
            let bot_color = grid[bot_sub * iw + col];

            let x = inner.x + col as u16;
            let y = inner.y + cell_row as u16;

            let cell = match buf.cell_mut((x, y)) {
                Some(c) => c,
                None => continue,
            };

            match (top_color, bot_color) {
                (None, None) => {
                    if let Some(bg) = bg_color {
                        cell.set_symbol(" ");
                        cell.set_style(ratatui::style::Style::default().bg(bg));
                    }
                }
                (Some(tc), None) => {
                    cell.set_symbol("▀");
                    let mut style = ratatui::style::Style::default().fg(tc);
                    if let Some(bg) = bg_color {
                        style = style.bg(bg);
                    }
                    cell.set_style(style);
                }
                (None, Some(bc)) => {
                    cell.set_symbol("▄");
                    let mut style = ratatui::style::Style::default().fg(bc);
                    if let Some(bg) = bg_color {
                        style = style.bg(bg);
                    }
                    cell.set_style(style);
                }
                (Some(tc), Some(bc)) => {
                    cell.set_symbol("▄");
                    cell.set_style(ratatui::style::Style::default().fg(bc).bg(tc));
                }
            }
        }
    }

    let vp_col_min = world_to_col(vp_x[0].max(wx_min));
    let vp_col_max = world_to_col(vp_x[1].min(wx_max));
    let vp_row_min = world_to_row(vp_y[1].min(wy_max));
    let vp_row_max = world_to_row(vp_y[0].max(wy_min));

    if vp_col_min >= vp_col_max || vp_row_min >= vp_row_max {
        return;
    }

    let vp_style = ratatui::style::Style::default().fg(params.colors.minimap_viewport_color);

    for col in vp_col_min..=vp_col_max {
        let x = inner.x + col as u16;

        let y_top = inner.y + vp_row_min as u16;
        if let Some(cell) = buf.cell_mut((x, y_top)) {
            cell.set_symbol("─");
            cell.set_style(vp_style);
        }

        let y_bot = inner.y + vp_row_max as u16;
        if let Some(cell) = buf.cell_mut((x, y_bot)) {
            cell.set_symbol("─");
            cell.set_style(vp_style);
        }
    }

    for row in vp_row_min..=vp_row_max {
        let y = inner.y + row as u16;

        let x_left = inner.x + vp_col_min as u16;
        if let Some(cell) = buf.cell_mut((x_left, y)) {
            cell.set_symbol("│");
            cell.set_style(vp_style);
        }

        let x_right = inner.x + vp_col_max as u16;
        if let Some(cell) = buf.cell_mut((x_right, y)) {
            cell.set_symbol("│");
            cell.set_style(vp_style);
        }
    }

    let corners: [(usize, usize, &str); 4] = [
        (vp_col_min, vp_row_min, "┌"),
        (vp_col_max, vp_row_min, "┐"),
        (vp_col_min, vp_row_max, "└"),
        (vp_col_max, vp_row_max, "┘"),
    ];
    for (col, row, sym) in corners {
        let x = inner.x + col as u16;
        let y = inner.y + row as u16;
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_symbol(sym);
            cell.set_style(vp_style);
        }
    }
}

fn compute_looking_glass_area(area: Rect, settings: &Settings, height: u16) -> Option<Rect> {
    let w = settings.visual.looking_glass_width;
    let minimap_w = settings.visual.minimap_width;
    if area.width < w.saturating_add(minimap_w).saturating_add(2) {
        return None;
    }
    if height < 4 {
        return None;
    }
    Some(Rect::new(area.x + 1, area.y + 1, w, height))
}

pub fn draw_looking_glass(
    frame: &mut ratatui::Frame,
    area: Rect,
    state: &GraphState,
    settings: &Settings,
    colors: &ThemeColors,
    cache: &RenderCache,
) {
    let Some(idx) = state.selection.primary else {
        return;
    };
    let graph = state.simulation.get_graph();
    let Some(node) = graph.node_weight(idx) else {
        return;
    };

    let bg = colors.background_color.unwrap_or(Color::Black);

    let node_color = cache
        .node_own_color
        .get(&idx)
        .copied()
        .unwrap_or(Color::Gray);

    // Tags render below the fixed-size visual; the glass grows downward.
    let tags: Vec<(String, Color)> = {
        let mut colors = Vec::new();
        let mut pal_idx = 0;
        for t in &node.data.tags {
            while pal_idx < LOCAL_TAG_PALETTE.len() && LOCAL_TAG_PALETTE[pal_idx] == node_color {
                pal_idx += 1;
            }
            let c = if pal_idx < LOCAL_TAG_PALETTE.len() {
                let color = LOCAL_TAG_PALETTE[pal_idx];
                pal_idx += 1;
                color
            } else {
                Color::White
            };
            colors.push((t.clone(), c));
        }
        colors
    };

    // Fixed visual height = the configured looking_glass_height (border
    // included). The link-count line + tag list extend the glass downward.
    let base_h = settings.visual.looking_glass_height;
    let meta_h = 1u16;
    let max_tags = area
        .height
        .saturating_sub(1)
        .saturating_sub(base_h)
        .saturating_sub(meta_h) as usize;
    let tag_count = tags.len().min(max_tags);
    let overlay_h = base_h
        .saturating_add(meta_h)
        .saturating_add(tag_count as u16)
        .min(area.height.saturating_sub(1));
    let Some(overlay) = compute_looking_glass_area(area, settings, overlay_h) else {
        return;
    };

    let title = truncate_ellipsis(&node.data.title, overlay.width.saturating_sub(4) as usize);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.minimap_border_color))
        .style(Style::default().bg(bg))
        .title(ratatui::text::Line::from(ratatui::text::Span::styled(
            format!(" {title} "),
            Style::default().fg(colors.label_color),
        )));
    let inner = block.inner(overlay);
    frame.render_widget(Clear, overlay);
    frame.render_widget(block, overlay);
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    // The node visual keeps its configured size; the footer (link count +
    // tags) occupies whatever remains below it.
    let visual_inner_h = base_h.saturating_sub(2).min(inner.height);
    let glass_canvas_area = Rect::new(inner.x, inner.y, inner.width, visual_inner_h);

    // Radius matches the simulation's node-size computation exactly.
    let radius = node_world_radius(settings, cache.max_link_count, node.data.link_count);
    let extra_tag_colors: Vec<Color> = tags.iter().map(|(_, c)| *c).collect();

    let filled = match settings.visual.node_fill {
        NodeFill::None => false,
        NodeFill::Filled => true,
        NodeFill::Dynamic => !matches!(settings.visual.node_scale, NodeScale::Fixed(1)),
    };
    let halo_offset = if filled && !extra_tag_colors.is_empty() {
        radius * 1.25
    } else {
        0.0
    };

    let node_render = NodeRenderData {
        filled,
        dimmed: false,
        x: 0.0,
        y: 0.0,
        color: node_color,
        radius,
        extra_tag_colors,
        grow_ring: false,
        is_selected: false,
        is_hovered: false,
        selection_ring_color: colors.selected_indicator_color,
        shape: settings.visual.node_shape,
    };

    // Bounds fit the node + tag orbit + selection ring, with the same
    // terminal-aspect correction the main canvas uses. Ensure both dimensions
    // fit without clipping.
    let aspect = glass_canvas_area.width as f64 / glass_canvas_area.height as f64;
    let required_extent = radius + halo_offset + 2.0;

    // Compute the minimum half_h needed so both h and w fit required_extent.
    let mut half_h = required_extent;
    let mut half_w = half_h * crate::viewport::CELL_ASPECT * aspect;

    if half_w < required_extent {
        half_h = required_extent / (crate::viewport::CELL_ASPECT * aspect);
        half_w = required_extent;
    }
    let glass_cell_w = (2.0 * half_w) / (glass_canvas_area.width as f64).max(1.0);
    let glass_cell_h = (2.0 * half_h) / (glass_canvas_area.height as f64).max(1.0);
    let glass_fill_step = fill_step_for(glass_cell_w, glass_cell_h);

    let canvas = Canvas::default()
        .background_color(bg)
        .marker(ratatui::symbols::Marker::from(
            settings.visual.canvas_marker,
        ))
        .x_bounds([-half_w, half_w])
        .y_bounds([-half_h, half_h])
        .paint(|ctx| {
            ctx.draw(&GraphNodesShape {
                nodes: std::slice::from_ref(&node_render),
                fill_step: glass_fill_step,
            });
        });
    frame.render_widget(canvas, glass_canvas_area);
    let footer_y = inner.y + visual_inner_h;
    let footer_h = inner.height.saturating_sub(visual_inner_h);
    if footer_h == 0 {
        return;
    }
    let link_label = if node.data.link_count == 1 {
        "1 link".to_string()
    } else {
        format!("{} links", node.data.link_count)
    };
    frame.render_widget(
        Paragraph::new(ratatui::text::Line::from(ratatui::text::Span::styled(
            link_label,
            Style::default().fg(colors.label_color),
        )))
        .style(Style::default().bg(bg)),
        Rect::new(inner.x, footer_y, inner.width, meta_h.min(footer_h)),
    );
    let tags_h = tag_count as u16;
    let avail_tags_h = footer_h.saturating_sub(meta_h);
    if tags_h > 0 && avail_tags_h > 0 {
        let tags_rect = Rect::new(
            inner.x,
            footer_y + meta_h,
            inner.width,
            tags_h.min(avail_tags_h),
        );
        let lines: Vec<ratatui::text::Line> = tags
            .iter()
            .take(tag_count)
            .map(|(tag, color)| {
                let label = truncate_ellipsis(tag, inner.width.saturating_sub(2) as usize);
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!("#{label}"),
                    Style::default().fg(*color),
                ))
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(bg)),
            tags_rect,
        );
    }
}

/// Marquee fill color: dimmed selection accent so the box reads as translucent.
fn muted_fill(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r / 4, g / 4, b / 4),
        _ => Color::DarkGray,
    }
}

/// Fills `rect` with `fill` preserving every underlying glyph and foreground.
fn draw_rect_filled(frame: &mut ratatui::Frame, rect: Rect, fill: Color) {
    let buf = frame.buffer_mut();
    for row in rect.y..rect.y.saturating_add(rect.height) {
        for col in rect.x..rect.x.saturating_add(rect.width) {
            if let Some(cell) = buf.cell_mut((col, row)) {
                cell.set_bg(fill);
            }
        }
    }
}

fn render_context_menu(
    frame: &mut ratatui::Frame,
    area: Rect,
    menu: &ContextMenu,
    theme: &ThemeColors,
    mouse_pos: Option<(u16, u16)>,
) {
    let rect = menu.rect(area);

    let bg_color = theme
        .menu_bg_color
        .or(theme.background_color)
        .unwrap_or(ratatui::style::Color::Reset);

    frame.render_widget(Clear, rect);
    let items: Vec<ListItem> = menu
        .items
        .iter()
        .enumerate()
        .map(|(i, spec)| {
            let is_selected = i == menu.selected;
            let base = if is_selected {
                let mut st = ratatui::style::Style::default().add_modifier(Modifier::BOLD);
                if let Some(c) = theme.highlight_fg {
                    st = st.fg(c);
                } else {
                    st = st.fg(bg_color);
                }
                if let Some(c) = theme.highlight_bg {
                    st = st.bg(c);
                } else {
                    st = st.bg(theme.label_color);
                }
                st
            } else {
                ratatui::style::Style::default().fg(theme.label_color)
            };
            let mut spans: Vec<ratatui::text::Span> = Vec::new();
            spans.push(ratatui::text::Span::styled("  ", base));
            if let Some(c) = spec.color_hint {
                spans.push(ratatui::text::Span::styled("■ ", base.fg(c)));
            }
            let label = format!("{}  ", spec.label);
            spans.push(ratatui::text::Span::styled(label, base));
            // dynamic padding so shortcut right-aligns.
            let content_len = spec.label.chars().count()
                + 4
                + usize::from(spec.color_hint.is_some()) * 2
                + usize::from(spec.shortcut.is_some()) * 2;
            let pad = (rect.width as usize).saturating_sub(content_len);
            if pad > 0 {
                spans.push(ratatui::text::Span::styled(" ".repeat(pad), base));
            }
            if let Some(c) = spec.shortcut {
                spans.push(ratatui::text::Span::styled(
                    format!("{c} "),
                    base.fg(theme.menu_shortcut_color.unwrap_or(theme.grid_color)),
                ));
            }
            ListItem::new(ratatui::text::Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(ratatui::widgets::Borders::NONE)
            .style(ratatui::style::Style::default().bg(bg_color)),
    );
    frame.render_widget(list, rect);

    // Hover highlight on the row under the mouse (excluding selected row).
    if let Some((col, row)) = mouse_pos
        && !rect.is_empty()
        && col >= rect.x
        && col < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
    {
        let idx = (row - rect.y) as usize;
        if idx < menu.items.len() && idx != menu.selected {
            let hover_rect = Rect::new(rect.x, row, rect.width, 1);
            let buf = frame.buffer_mut();
            for c in hover_rect.left()..hover_rect.right() {
                if let Some(cell) = buf.cell_mut((c, row)) {
                    if let Some(hbg) = theme.highlight_bg {
                        cell.set_bg(hbg);
                    }
                    if let Some(hfg) = theme.highlight_fg {
                        cell.set_fg(hfg);
                    }
                }
            }
        }
    }
}

// ── Adaptive grid overlay ─────────────────────────────────────────────────────

/// Affine world-to-terminal projection used by [`draw_canvas_grid`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct GridProjection {
    world_left: f64,
    world_right: f64,
    world_top: f64,
    world_bottom: f64,
    origin_col: f64,
    origin_row: f64,
    cols_per_world_x: f64,
    rows_per_world_y: f64,
}

impl GridProjection {
    fn is_valid(self) -> bool {
        [
            self.world_left,
            self.world_right,
            self.world_top,
            self.world_bottom,
            self.origin_col,
            self.origin_row,
            self.cols_per_world_x,
            self.rows_per_world_y,
        ]
        .into_iter()
        .all(f64::is_finite)
            && self.cols_per_world_x != 0.0
            && self.rows_per_world_y != 0.0
    }
}

/// Draw adaptive grid dots before view content so later view rendering replaces them.
fn draw_canvas_grid(
    frame: &mut ratatui::Frame,
    area: Rect,
    visible: bool,
    projection: GridProjection,
    muted: Color,
    zoom: f64,
) {
    if !visible || area.is_empty() || !projection.is_valid() || !zoom.is_finite() || zoom <= 0.0 {
        return;
    }

    let min_x = projection.world_left.min(projection.world_right);
    let max_x = projection.world_left.max(projection.world_right);
    let min_y = projection.world_top.min(projection.world_bottom);
    let max_y = projection.world_top.max(projection.world_bottom);
    let mut grid_step_x: f64 = 100.0;
    let mut grid_step_y: f64 = 100.0;
    while grid_step_y * zoom < 6.0 {
        grid_step_x *= 2.0;
        grid_step_y *= 2.0;
    }
    // Compensate for terminal cell aspect ratio (~2:1 height:width) so grid appears square
    grid_step_y *= projection.cols_per_world_x.abs() / (2.0 * projection.rows_per_world_y.abs());
    let step_x = grid_step_x;
    let step_y = grid_step_y;
    if !step_x.is_finite() || !step_y.is_finite() || step_x == 0.0 || step_y == 0.0 {
        return;
    }

    let Some(start_x) = grid_index(min_x, step_x, f64::floor) else {
        return;
    };
    let Some(end_x) = grid_index(max_x, step_x, f64::ceil) else {
        return;
    };
    let Some(start_y) = grid_index(min_y, step_y, f64::floor) else {
        return;
    };
    let Some(end_y) = grid_index(max_y, step_y, f64::ceil) else {
        return;
    };

    let width = i64::from(area.width);
    let height = i64::from(area.height);
    let max_dots = width.saturating_mul(height).saturating_mul(4).max(1);
    let x_count = end_x.saturating_sub(start_x).saturating_add(1);
    let y_count = end_y.saturating_sub(start_y).saturating_add(1);
    if x_count.saturating_mul(y_count) > max_dots {
        return;
    }

    let left = f64::from(area.left());
    let right = f64::from(area.right());
    let top = f64::from(area.top());
    let bottom = f64::from(area.bottom());
    let buffer = frame.buffer_mut();
    for x_index in start_x..=end_x {
        let world_x = x_index as f64 * step_x;
        let col = projection.origin_col + world_x * projection.cols_per_world_x;
        if !col.is_finite() {
            continue;
        }
        let col = col.round();
        if col < left || col >= right {
            continue;
        }
        for y_index in start_y..=end_y {
            let world_y = y_index as f64 * step_y;
            let row = projection.origin_row + world_y * projection.rows_per_world_y;
            if !row.is_finite() {
                continue;
            }
            let row = row.round();
            if row < top || row >= bottom {
                continue;
            }
            if let Some(cell) = buffer.cell_mut((col as u16, row as u16))
                && (cell.symbol() == " " || cell.symbol() == "")
            {
                cell.set_char('·').set_fg(muted);
            }
        }
    }
}

fn grid_index(value: f64, step: f64, round: fn(f64) -> f64) -> Option<i64> {
    let index = round(value / step);
    (index >= i64::MIN as f64 && index <= i64::MAX as f64).then_some(index as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphNodeData;
    use fdg_sim::{ForceGraph, ForceGraphHelper};

    // Generous bounds covering all nodes in test graphs
    const TEST_X_BOUNDS: [f64; 2] = [-1000.0, 1000.0];
    const TEST_Y_BOUNDS: [f64; 2] = [-1000.0, 1000.0];

    #[test]
    fn test_fill_labels() {
        let mut graph: ForceGraph<GraphNodeData, ()> = ForceGraph::default();

        let n1_data = GraphNodeData {
            id: "1".to_string(),
            title: "Node 1".to_string(),
            tags: vec![],
            link_count: 0,
            folder: "".to_string(),
        };
        let n2_data = GraphNodeData {
            id: "2".to_string(),
            title: "Node 2".to_string(),
            tags: vec![],
            link_count: 0,
            folder: "".to_string(),
        };
        let n3_data = GraphNodeData {
            id: "3".to_string(),
            title: "Node 3".to_string(),
            tags: vec![],
            link_count: 0,
            folder: "".to_string(),
        };

        let idx1 = graph.add_force_node("Node 1", n1_data);
        let idx2 = graph.add_force_node("Node 2", n2_data);
        let _idx3 = graph.add_force_node("Node 3", n3_data);

        // Add edge: idx1 - idx2 (idx3 is isolated)
        graph.add_edge(idx1, idx2, ());

        let mut cache = RenderCache::new();
        let mut settings = Settings::default();
        let selected_nodes = std::collections::HashSet::new();

        // 1. LabelMode::None
        settings.visual.label_mode = LabelMode::None;
        let _tier = cache.fill_nodes(
            &graph,
            &settings,
            Some(idx1),
            &selected_nodes,
            &selected_nodes,
            ratatui::style::Color::Red,
            None,
            TEST_X_BOUNDS,
            TEST_Y_BOUNDS,
            0.0,
        );
        cache.fill_labels(&graph, &settings, Some(idx1), &selected_nodes, 0.0, _tier);
        assert!(cache.labels.is_empty());

        // 2. LabelMode::All
        settings.visual.label_mode = LabelMode::All;
        let _tier = cache.fill_nodes(
            &graph,
            &settings,
            Some(idx1),
            &selected_nodes,
            &selected_nodes,
            ratatui::style::Color::Red,
            None,
            TEST_X_BOUNDS,
            TEST_Y_BOUNDS,
            0.0,
        );
        cache.fill_labels(&graph, &settings, Some(idx1), &selected_nodes, 0.0, _tier);
        assert_eq!(cache.labels.len(), 3);

        // 3. LabelMode::Selected
        settings.visual.label_mode = LabelMode::Selected;
        let _tier = cache.fill_nodes(
            &graph,
            &settings,
            Some(idx1),
            &selected_nodes,
            &selected_nodes,
            ratatui::style::Color::Red,
            None,
            TEST_X_BOUNDS,
            TEST_Y_BOUNDS,
            0.0,
        );
        cache.fill_labels(&graph, &settings, Some(idx1), &selected_nodes, 0.0, _tier);
        assert_eq!(cache.labels.len(), 1);
        assert_eq!(
            cache.label_texts.get(&cache.labels[0].node_idx).unwrap(),
            "Node 1"
        );

        // 4. LabelMode::Neighbors
        settings.visual.label_mode = LabelMode::Neighbors;
        let _tier = cache.fill_nodes(
            &graph,
            &settings,
            Some(idx1),
            &selected_nodes,
            &selected_nodes,
            ratatui::style::Color::Red,
            None,
            TEST_X_BOUNDS,
            TEST_Y_BOUNDS,
            0.0,
        );
        cache.fill_labels(&graph, &settings, Some(idx1), &selected_nodes, 0.0, _tier);
        // Node 1 (selected) and Node 2 (neighbor) should have labels. Node 3 (distant) should not.
        assert_eq!(cache.labels.len(), 2);
        let mut names: Vec<String> = cache
            .labels
            .iter()
            .map(|l| cache.label_texts.get(&l.node_idx).unwrap().clone())
            .collect();
        names.sort();
        assert_eq!(names, vec!["Node 1".to_string(), "Node 2".to_string()]);

        // 5. Test min_offset_y parameter
        let _tier = cache.fill_nodes(
            &graph,
            &settings,
            Some(idx1),
            &selected_nodes,
            &selected_nodes,
            ratatui::style::Color::Red,
            None,
            TEST_X_BOUNDS,
            TEST_Y_BOUNDS,
            0.0,
        );
        settings.visual.label_mode = LabelMode::Selected;
        cache.fill_labels(&graph, &settings, Some(idx1), &selected_nodes, 10.0, _tier);
        assert_eq!(cache.labels.len(), 1);
        let label = &cache.labels[0];
        let node_y = graph[idx1].location.y as f64;
        let radius = cache
            .nodes
            .get(idx1.index())
            .map(|n| n.radius)
            .unwrap_or(2.0);
        // The default label_offset is 4.0, but min_offset_y is 10.0. The actual offset should be 10.0.
        assert_eq!(label.y, node_y + radius + 10.0);

        cache.fill_labels(&graph, &settings, Some(idx1), &selected_nodes, 1.0, _tier);
        let label = &cache.labels[0];
        // The default label_offset is 4.0, which is larger than min_offset_y of 1.0. The actual offset should be 4.0.
        assert_eq!(label.y, node_y + radius + 4.0);
    }
}

#[cfg(test)]
mod node_scale_tests {
    use super::node_floor_rows;
    use crate::settings::NodeScale;

    #[test]
    fn automatic_fades_from_large_to_small_with_vault_size() {
        assert_eq!(node_floor_rows(NodeScale::Fixed(1), 10), 0.0);
        assert!((node_floor_rows(NodeScale::Fixed(10), 5_000) - 0.9).abs() < 1e-9);
        let small_vault = node_floor_rows(NodeScale::Automatic, 20);
        let mid_vault = node_floor_rows(NodeScale::Automatic, 250);
        let big_vault = node_floor_rows(NodeScale::Automatic, 1_000);
        assert!((small_vault - node_floor_rows(NodeScale::Fixed(10), 20)).abs() < 1e-9);
        assert!(mid_vault > 0.0 && mid_vault < small_vault);
        assert_eq!(big_vault, 0.0);
    }

    #[test]
    fn fixed_scale_spans_small_to_large_linearly() {
        assert_eq!(node_floor_rows(NodeScale::Fixed(1), 10), 0.0);
        assert!((node_floor_rows(NodeScale::Fixed(10), 5_000) - 0.9).abs() < 1e-9);
        assert!((node_floor_rows(NodeScale::Fixed(5), 100) - 0.9 * 4.0 / 9.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod dim_tests {
    use super::*;
    use crate::graph::NodeSpec;
    use crate::theme::theme_colors;

    fn lit_set(
        graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
        sel: NodeIndex,
    ) -> HashSet<NodeIndex> {
        let mut lit = HashSet::new();
        lit.insert(sel);
        for edge in graph.edges(sel) {
            lit.insert(edge.source());
            lit.insert(edge.target());
        }
        lit
    }

    fn build(
        specs: &[NodeSpec],
    ) -> (
        GraphState,
        Settings,
        RenderCache,
        std::collections::HashMap<String, NodeIndex>,
    ) {
        let mut settings = Settings::default();
        settings.filter.show_orphan = true;
        settings.visual.selection_focus = SelectionFocus::Dim;
        let state = GraphState::from_specs(specs, &settings).unwrap();
        let graph = state.simulation.get_graph();
        let ids: std::collections::HashMap<String, NodeIndex> = graph
            .node_indices()
            .map(|i| (graph[i].data.id.clone(), i))
            .collect();
        let mut cache = RenderCache::new();
        cache.rebuild_topology(
            graph,
            &settings,
            &theme_colors(&settings.visual.theme, settings.visual.background.clone()),
            false,
        );
        (state, settings, cache, ids)
    }

    #[test]
    fn dim_grays_everything_outside_selection_neighborhood() {
        let specs = vec![
            NodeSpec {
                id: "a".into(),
                title: "a".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["b".into()],
            },
            NodeSpec {
                id: "b".into(),
                title: "b".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["a".into(), "c".into()],
            },
            NodeSpec {
                id: "c".into(),
                title: "c".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["b".into()],
            },
            NodeSpec {
                id: "x".into(),
                title: "x".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["y".into()],
            },
            NodeSpec {
                id: "y".into(),
                title: "y".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["x".into()],
            },
            NodeSpec {
                id: "z".into(),
                title: "z".into(),
                tags: vec![],
                folder: String::new(),
                links: vec![],
            },
        ];
        let (mut state, settings, mut cache, ids) = build(&specs);
        let b = ids["b"];
        state.selection.primary = Some(b);
        let selected_set: HashSet<NodeIndex> = HashSet::from([b]);
        let lit = lit_set(state.simulation.get_graph(), b);

        let graph = state.simulation.get_graph();
        let tier = cache.fill_nodes(
            graph,
            &settings,
            Some(b),
            &selected_set,
            &lit,
            Color::White,
            None,
            [-1e9, 1e9],
            [-1e9, 1e9],
            1.0,
        );
        cache.fill_edges(graph, &settings, Color::White, tier, &lit);

        // a, b, c lit; x, y, z dimmed.
        assert_eq!(
            cache
                .nodes
                .iter()
                .filter(|n| n.color == Color::DarkGray)
                .count(),
            3
        );
        assert_eq!(
            cache
                .nodes
                .iter()
                .filter(|n| n.color != Color::DarkGray)
                .count(),
            3
        );
        assert!(
            cache
                .nodes
                .iter()
                .all(|n| n.dimmed == (n.color == Color::DarkGray))
        );
        // Only the x-y edge (both endpoints outside lit) dims.
        assert_eq!(
            cache
                .edges
                .iter()
                .filter(|e| e.color == Color::DarkGray)
                .count(),
            1
        );
        assert_eq!(cache.edges.len(), 3);
    }

    #[test]
    fn edge_between_two_lit_neighbors_stays_colored() {
        let specs = vec![
            NodeSpec {
                id: "a".into(),
                title: "a".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["b".into(), "c".into()],
            },
            NodeSpec {
                id: "b".into(),
                title: "b".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["a".into(), "c".into()],
            },
            NodeSpec {
                id: "c".into(),
                title: "c".into(),
                tags: vec![],
                folder: String::new(),
                links: vec!["b".into(), "a".into()],
            },
        ];
        let (mut state, settings, mut cache, ids) = build(&specs);
        let b = ids["b"];
        state.selection.primary = Some(b);
        let selected_set: HashSet<NodeIndex> = HashSet::from([b]);
        let lit = lit_set(state.simulation.get_graph(), b);

        let graph = state.simulation.get_graph();
        let tier = cache.fill_nodes(
            graph,
            &settings,
            Some(b),
            &selected_set,
            &lit,
            Color::White,
            None,
            [-1e9, 1e9],
            [-1e9, 1e9],
            1.0,
        );
        cache.fill_edges(graph, &settings, Color::White, tier, &lit);

        // a-b, b-c touch the selection; a-c has both endpoints lit -> stays colored.
        assert_eq!(cache.edges.len(), 3);
        assert!(
            cache.edges.iter().all(|e| e.color != Color::DarkGray),
            "all edges are within the lit neighborhood"
        );
    }
}

#[cfg(test)]
mod fill_tests {
    use super::fill_step_for;

    #[test]
    fn fill_step_is_quarter_of_smaller_cell_dimension() {
        assert_eq!(fill_step_for(2.0, 4.0), 0.5);
        assert_eq!(fill_step_for(4.0, 2.0), 0.5);
    }
}
