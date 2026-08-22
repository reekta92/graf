use ratatui::layout::Rect;

use fdg_sim::petgraph::graph::NodeIndex;

use crate::graph::GraphState;
use crate::settings::{NodeSizeMode, Settings};

pub const CANVAS_ZOOM_MIN: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomDir {
    In,
    Out,
}

pub const CELL_ASPECT: f64 = 0.5;
/// Lowest zoom-out permitted, expressed as a fraction of `auto_fit_zoom`
/// (i.e. `scale() >= MIN_SCALE`). Bounds screen_to_world so node-drag never
/// writes coordinates large enough to destabilise the force simulation.
const MIN_SCALE: f64 = 0.15;

/// Visual-row slop added around a node's drawn body, and the minimum click
/// radius in screen rows so sub-pixel (zoomed-out) nodes stay clickable.
const HIT_SLOP_ROWS: f64 = 1.5;
const HIT_MIN_ROWS: f64 = 2.0;

/// World-space radius of a node, identical to the radius used for drawing.
/// Single source of truth for `fill_nodes`, the looking-glass preview, and
/// `Viewport::hit_test`.
pub fn node_world_radius(settings: &Settings, max_link_count: usize, link_count: usize) -> f64 {
    match settings.visual.node_size_mode {
        NodeSizeMode::Fixed => settings.visual.node_size,
        NodeSizeMode::LinkCount => {
            if max_link_count == 0 {
                settings.visual.node_size
            } else {
                settings.visual.node_size
                    * (1.0 + (link_count as f64 / max_link_count as f64) * 1.5)
            }
        }
    }
}

#[derive(Clone)]
pub struct Viewport {
    pub center_x: f64,
    pub center_y: f64,
    pub zoom: f64,
    pub auto_fit_zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_y: 0.0,
            zoom: 1.0,
            auto_fit_zoom: 1.0,
        }
    }
}

impl Viewport {
    pub fn x_bounds(&self, aspect: f64) -> [f64; 2] {
        let half_w = (100.0 * CELL_ASPECT * CELL_ASPECT * aspect) / self.zoom;
        [self.center_x - half_w, self.center_x + half_w]
    }

    pub fn y_bounds(&self, _aspect: f64) -> [f64; 2] {
        let half_h = 100.0 * CELL_ASPECT / self.zoom;
        [self.center_y - half_h, self.center_y + half_h]
    }

    pub fn screen_to_world(&self, col: u16, row: u16, area: Rect) -> (f64, f64) {
        let aspect = area.width as f64 / area.height as f64;
        let [x_left, x_right] = self.x_bounds(aspect);
        let [y_bottom, y_top] = self.y_bounds(aspect);

        let wx = x_left + ((col as f64 - area.x as f64) / area.width as f64) * (x_right - x_left);
        let wy = y_top - ((row as f64 - area.y as f64) / area.height as f64) * (y_top - y_bottom);
        (clamp_world(wx), clamp_world(wy))
    }

    pub fn world_to_screen(&self, wx: f64, wy: f64, area: Rect) -> (f64, f64) {
        let aspect = area.width as f64 / area.height as f64;
        let [x_left, x_right] = self.x_bounds(aspect);
        let [y_bottom, y_top] = self.y_bounds(aspect);

        let col = area.x as f64 + ((wx - x_left) / (x_right - x_left)) * area.width as f64;
        let row = area.y as f64 + ((y_top - wy) / (y_top - y_bottom)) * area.height as f64;
        (col, row)
    }

    #[must_use]
    pub fn auto_fit_from_graph(
        &self,
        graph: &fdg_sim::ForceGraph<crate::graph::GraphNodeData, ()>,
        auto_fit_padding: f64,
    ) -> Viewport {
        let mut vp = self.clone();
        if graph.node_count() == 0 {
            return Viewport::default();
        }

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

        vp.center_x = (min_x + max_x) / 2.0;
        vp.center_y = (min_y + max_y) / 2.0;

        let range_x = (max_x - min_x).max(1.0);
        let range_y = (max_y - min_y).max(1.0);
        let range = range_x.max(range_y) * auto_fit_padding;
        let full_zoom = 200.0 / range;
        vp.zoom = full_zoom;
        vp.auto_fit_zoom = full_zoom;
        vp
    }

    pub fn scale(&self) -> f64 {
        self.zoom / self.auto_fit_zoom
    }

    pub fn zoom_in(&mut self, factor: f64) {
        if let Some(z) = zoom_step(self.zoom, factor, ZoomDir::In, 0.0) {
            self.zoom = z;
        }
    }

    pub fn zoom_out(&mut self, factor: f64) {
        let min = MIN_SCALE * self.auto_fit_zoom;
        if let Some(z) = zoom_step(self.zoom, factor, ZoomDir::Out, min) {
            self.zoom = z;
        }
    }

    pub fn center_on_node(&mut self, x: f32, y: f32) {
        let x_f64 = x as f64;
        let y_f64 = y as f64;
        if x_f64.is_finite() && y_f64.is_finite() {
            self.center_x = x_f64;
            self.center_y = y_f64;
        }
    }

    pub fn set_center(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.center_x = x;
            self.center_y = y;
        }
    }

    pub fn pan_by(&mut self, dx: f64, dy: f64) {
        if let Some((nx, ny)) = pan_centered(self.center_x, self.center_y, dx, dy) {
            self.center_x = nx;
            self.center_y = ny;
        }
    }

    pub fn nearest_to_center(&self, state: &GraphState) -> Option<NodeIndex> {
        let graph = state.simulation.get_graph();
        let ids: Vec<NodeIndex> = graph.node_indices().collect();
        let cands: Vec<(f64, f64)> = ids
            .iter()
            .map(|&i| {
                let node = &graph[i];
                (node.location.x as f64, node.location.y as f64)
            })
            .collect();
        nearest_to_point(&cands, (self.center_x, self.center_y)).map(|i| ids[i])
    }

    pub fn nearest_in_direction(
        &self,
        state: &GraphState,
        origin_x: f64,
        origin_y: f64,
        dir_x: f64,
        dir_y: f64,
        exclude: Option<NodeIndex>,
    ) -> Option<NodeIndex> {
        let graph = state.simulation.get_graph();
        let mut ids: Vec<NodeIndex> = Vec::new();
        let mut cands: Vec<(f64, f64)> = Vec::new();
        for idx in graph.node_indices() {
            if exclude == Some(idx) {
                continue;
            }
            let node = &graph[idx];
            ids.push(idx);
            cands.push((node.location.x as f64, node.location.y as f64));
        }
        nearest_in_dir(
            &cands,
            (origin_x, origin_y),
            (dir_x, dir_y),
            std::f64::consts::FRAC_PI_3,
        )
        .map(|i| ids[i])
    }

    pub fn hit_test(
        &self,
        world_x: f64,
        world_y: f64,
        state: &GraphState,
        settings: &Settings,
        area: Rect,
        max_link_count: usize,
    ) -> Option<NodeIndex> {
        if !world_x.is_finite() || !world_y.is_finite() {
            return None;
        }
        let h = (area.height as f64).max(1.0);
        let world_per_row = (100.0 / self.zoom) / h;
        let pad_world = HIT_SLOP_ROWS * world_per_row;
        let min_hit_world = HIT_MIN_ROWS * world_per_row;

        let max_node_radius_world = settings.visual.node_size * 2.5;
        let query = max_node_radius_world + pad_world + min_hit_world;

        let graph = state.simulation.get_graph();
        let mut contained: Option<(NodeIndex, f64)> = None;
        let mut near: Option<(NodeIndex, f64)> = None;

        for idx in crate::graph::nodes_in_rect(
            graph,
            world_x - query,
            world_y - query,
            world_x + query,
            world_y + query,
        ) {
            let node = &graph[idx];
            let dx = node.location.x as f64 - world_x;
            let dy = node.location.y as f64 - world_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if !dist.is_finite() {
                continue;
            }
            let nr = node_world_radius(settings, max_link_count, node.data.link_count);
            let click_thresh = (nr + pad_world).max(min_hit_world);
            if dist <= nr {
                match contained {
                    Some((bi, bd)) if dist >= bd && !(dist == bd && idx.index() < bi.index()) => {}
                    _ => contained = Some((idx, dist)),
                }
            }
            if dist <= click_thresh {
                match near {
                    Some((bi, bd)) if dist >= bd && !(dist == bd && idx.index() < bi.index()) => {}
                    _ => near = Some((idx, dist)),
                }
            }
        }

        contained.or(near).map(|(idx, _)| idx)
    }
}

/// Clamps a world coordinate to a finite range; non-finite → 0.
pub fn clamp_world(v: f64) -> f64 {
    const WORLD_COORD_LIMIT: f64 = 1.0e18;
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(-WORLD_COORD_LIMIT, WORLD_COORD_LIMIT)
}

/// Returns None on non-finite → caller leaves center unchanged.
pub fn pan_centered(cx: f64, cy: f64, dx: f64, dy: f64) -> Option<(f64, f64)> {
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }
    let (nx, ny) = (cx + dx, cy + dy);
    if nx.is_finite() && ny.is_finite() {
        Some((nx, ny))
    } else {
        None
    }
}

/// In: zoom*factor. Out: zoom/factor floored at `min`. Rejects non-finite.
pub fn zoom_step(zoom: f64, factor: f64, dir: ZoomDir, min: f64) -> Option<f64> {
    if !factor.is_finite() || factor <= 0.0 || !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }
    let candidate = match dir {
        ZoomDir::In => zoom * factor,
        ZoomDir::Out => zoom / factor,
    };
    if !candidate.is_finite() || candidate <= 0.0 || !(100.0 / candidate).is_finite() {
        return None;
    }
    let floored = if matches!(dir, ZoomDir::Out) && min.is_finite() && min > 0.0 {
        candidate.max(min)
    } else {
        candidate
    };
    Some(floored)
}

/// 60° cone forward search. `cands` = candidate positions in view
/// iteration order (caller excludes the current node BEFORE building the
/// slice so ties resolve identically). Returns index into `cands`.
pub fn nearest_in_dir(
    cands: &[(f64, f64)],
    origin: (f64, f64),
    dir: (f64, f64),
    cone: f64,
) -> Option<usize> {
    let dir_len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
    if dir_len == 0.0 {
        return None;
    }
    let (ndx, ndy) = (dir.0 / dir_len, dir.1 / dir_len);
    const ANGLE_WEIGHT: f64 = 80.0;
    let mut best: Option<(usize, f64)> = None;
    for (i, &(cx, cy)) in cands.iter().enumerate() {
        let (dx, dy) = (cx - origin.0, cy - origin.1);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1e-6 {
            continue;
        }
        let dot = (dx * ndx + dy * ndy) / dist;
        if dot < 0.0 {
            continue;
        }
        let angle = dot.acos();
        if angle > cone {
            continue;
        }
        let score = ANGLE_WEIGHT * angle + dist;
        match best {
            Some((_, bs)) if score >= bs => {}
            _ => best = Some((i, score)),
        }
    }
    best.map(|(i, _)| i)
}

/// Closest-by-Euclidean to a target point (no-selection fallback).
pub fn nearest_to_point(cands: &[(f64, f64)], target: (f64, f64)) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &(cx, cy)) in cands.iter().enumerate() {
        let d = ((cx - target.0).powi(2) + (cy - target.1).powi(2)).sqrt();
        match best {
            Some((_, bd)) if d >= bd => {}
            _ => best = Some((i, d)),
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphNodeData;
    use fdg_sim::{ForceGraph, ForceGraphHelper, Simulation, SimulationParameters};
    use std::f64::consts::FRAC_PI_3;

    // ── camera function tests ────────────────────────────────────────────────

    #[test]
    fn clamp_world_bounds() {
        assert_eq!(clamp_world(f64::NAN), 0.0);
        assert_eq!(clamp_world(f64::INFINITY), 0.0);
        assert_eq!(clamp_world(f64::NEG_INFINITY), 0.0);
        assert_eq!(clamp_world(1.0e19), 1.0e18);
        assert_eq!(clamp_world(-1.0e19), -1.0e18);
        assert_eq!(clamp_world(0.0), 0.0);
        assert_eq!(clamp_world(42.0), 42.0);
    }

    #[test]
    fn zoom_step_floors_out() {
        assert_eq!(zoom_step(1.0, 2.0, ZoomDir::In, 0.0), Some(2.0));
        // Out floors at min.
        assert_eq!(zoom_step(0.01, 2.0, ZoomDir::Out, 0.05), Some(0.05));
        assert_eq!(zoom_step(0.2, 2.0, ZoomDir::Out, 0.05), Some(0.1));
        // In ignores non-finite factor.
        assert_eq!(zoom_step(1.0, f64::NAN, ZoomDir::In, 0.0), None);
        // Out rejects factor <= 0.
        assert_eq!(zoom_step(1.0, 0.0, ZoomDir::Out, 0.05), None);
        assert_eq!(zoom_step(1.0, -1.0, ZoomDir::Out, 0.05), None);
        // Non-finite zoom rejected.
        assert_eq!(zoom_step(f64::NAN, 2.0, ZoomDir::In, 0.0), None);
    }

    #[test]
    fn pan_centered_rejects_nan() {
        assert_eq!(pan_centered(1.0, 2.0, f64::NAN, 0.0), None);
        assert_eq!(pan_centered(1.0, 2.0, 3.0, 4.0), Some((4.0, 6.0)));
    }

    #[test]
    fn nearest_in_dir_picks_forward() {
        // origin (0,0), dir (1,0): node at (1,0) forward, (0,1) orthogonal-out,
        // (-1,0) behind.
        let cands = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];
        assert_eq!(
            nearest_in_dir(&cands, (0.0, 0.0), (1.0, 0.0), FRAC_PI_3),
            Some(0)
        );
        // Behind node only → None.
        assert_eq!(
            nearest_in_dir(&[(-1.0, 0.0)], (0.0, 0.0), (1.0, 0.0), FRAC_PI_3),
            None
        );
        // Outside-cone: node at 90° > 60° cone → None.
        assert_eq!(
            nearest_in_dir(&[(0.0, 1.0)], (0.0, 0.0), (1.0, 0.0), FRAC_PI_3),
            None
        );
        // Empty → None.
        assert_eq!(nearest_in_dir(&[], (0.0, 0.0), (1.0, 0.0), FRAC_PI_3), None);
        // Tie → first index wins (both at distance 1 on-axis).
        assert_eq!(
            nearest_in_dir(&[(1.0, 0.0), (1.0, 0.0)], (0.0, 0.0), (1.0, 0.0), FRAC_PI_3),
            Some(0)
        );
    }

    #[test]
    fn nearest_to_point_closest_wins() {
        let cands = [(10.0, 10.0), (1.0, 1.0), (5.0, 5.0)];
        assert_eq!(nearest_to_point(&cands, (0.0, 0.0)), Some(1));
        assert_eq!(nearest_to_point(&[], (0.0, 0.0)), None);
    }

    // ── viewport tests (ported from clin) ─────────────────────────────────────

    #[test]
    fn test_viewport_baseline_scale() {
        let vp = Viewport::default();
        assert_eq!(vp.scale(), 1.0);
    }

    #[test]
    fn test_viewport_zoom_beyond_old_caps() {
        let mut vp = Viewport::default();
        vp.zoom_in(200.0);
        assert_eq!(vp.zoom, 200.0);
        assert_eq!(vp.scale(), 200.0);

        vp.zoom = 1.0;
        vp.zoom_out(100.0);
        assert!((vp.zoom - 0.15).abs() < 1e-12);
        assert!((vp.scale() - 0.15).abs() < 1e-12);
    }

    #[test]
    fn test_viewport_invalid_candidate_rejection() {
        let mut vp = Viewport::default();

        vp.zoom_in(f64::NAN);
        assert_eq!(vp.zoom, 1.0);

        vp.zoom_in(f64::INFINITY);
        assert_eq!(vp.zoom, 1.0);

        vp.zoom_out(0.0);
        assert_eq!(vp.zoom, 1.0);

        vp.zoom_out(-2.0);
        assert_eq!(vp.zoom, 1.0);
    }

    #[test]
    fn test_viewport_center_and_pan_guards() {
        let mut vp = Viewport::default();

        vp.set_center(f64::NAN, 0.0);
        assert_eq!(vp.center_x, 0.0);

        vp.set_center(0.0, f64::INFINITY);
        assert_eq!(vp.center_y, 0.0);

        vp.pan_by(f64::NAN, 1.0);
        assert_eq!(vp.center_x, 0.0);

        vp.pan_by(1.0, f64::NEG_INFINITY);
        assert_eq!(vp.center_y, 0.0);
    }

    fn make_state(nodes: &[(f64, f64, usize)]) -> (GraphState, Vec<NodeIndex>) {
        let mut graph: ForceGraph<GraphNodeData, ()> = ForceGraph::default();
        let mut idxs = Vec::new();
        for (i, &(x, y, lc)) in nodes.iter().enumerate() {
            let data = GraphNodeData {
                id: format!("{i}"),
                title: format!("Node {i}"),
                tags: vec![],
                link_count: lc,
                folder: "".to_string(),
            };
            let idx = graph.add_force_node(format!("Node {i}"), data);
            graph.node_weight_mut(idx).unwrap().location.x = x as f32;
            graph.node_weight_mut(idx).unwrap().location.y = y as f32;
            idxs.push(idx);
        }

        let mut gs = GraphState::new(
            Simulation::from_graph(graph, SimulationParameters::default()),
            Viewport::default(),
        );
        // `Simulation::from_graph` re-initialises node locations; re-apply the
        // explicit positions so the spatial grid reflects them.
        for (i, &idx) in idxs.iter().enumerate() {
            let (x, y, _) = nodes[i];
            let node = gs.simulation.get_graph_mut().node_weight_mut(idx).unwrap();
            node.location.x = x as f32;
            node.location.y = y as f32;
        }
        (gs, idxs)
    }

    #[test]
    fn test_hit_test_parity_with_brute_force() {
        let (gs, idxs) = make_state(&[
            (0.0, 0.0, 0),
            (10000.0, 10000.0, 0),
            (10005.0, 10005.0, 0),
            (-10000.0, -10000.0, 0),
        ]);

        let vp = Viewport {
            zoom: 1.0,
            ..Default::default()
        };

        let config = Settings::default();
        let area = Rect::new(0, 0, 80, 40);
        let max_lc = 0;

        let hit = vp
            .hit_test(10001.0, 10001.0, &gs, &config, area, max_lc)
            .unwrap();
        assert_eq!(hit, idxs[1]);

        let hit_equal = vp
            .hit_test(10002.5, 10002.5, &gs, &config, area, max_lc)
            .unwrap();
        assert_eq!(hit_equal, idxs[1]);
    }

    #[test]
    fn test_hit_test_containment_prefers_body_over_neighbor() {
        // A = large node (LinkCount max), B = small node; cursor sits inside A's
        // body but outside B's body, yet B's center is nearer. Containment-first
        // must return A; nearest-center (old behavior) would return B.
        let mut config = Settings::default();
        config.visual.node_size_mode = NodeSizeMode::LinkCount;
        // node_size 2.0: A (lc=4, max=4) → r=5.0 ; B (lc=0) → r=2.0.
        let (gs, idxs) = make_state(&[(0.0, 0.0, 4), (3.0, -2.5, 0)]);
        let vp = Viewport {
            zoom: 1.0,
            ..Default::default()
        };
        let area = Rect::new(0, 0, 80, 40);
        let max_lc = 4;

        assert_eq!(node_world_radius(&config, 4, 4), 5.0);
        assert_eq!(node_world_radius(&config, 4, 0), 2.0);
        let hit = vp.hit_test(3.0, 0.0, &gs, &config, area, max_lc).unwrap();
        assert_eq!(hit, idxs[0]);
    }

    #[test]
    fn test_hit_test_zoom_extremes() {
        let (gs, idxs) = make_state(&[(0.0, 0.0, 0), (10000.0, 10000.0, 0), (10005.0, 10005.0, 0)]);
        let config = Settings::default();
        let area = Rect::new(0, 0, 80, 40);
        let max_lc = 0;

        let vp_zoom_out = Viewport {
            zoom: 0.15,
            ..Default::default()
        };
        let vp_zoom_in = Viewport {
            zoom: 200.0,
            ..Default::default()
        };

        // Cursor on idx1's body (radius 2.0): (10001,10001) ~1.41 away.
        assert_eq!(
            vp_zoom_out.hit_test(10001.0, 10001.0, &gs, &config, area, max_lc),
            Some(idxs[1])
        );
        assert_eq!(
            vp_zoom_in.hit_test(10001.0, 10001.0, &gs, &config, area, max_lc),
            Some(idxs[1])
        );

        // Cursor far from every node → None at both extremes.
        assert_eq!(
            vp_zoom_out.hit_test(50000.0, 50000.0, &gs, &config, area, max_lc),
            None
        );
        assert_eq!(
            vp_zoom_in.hit_test(50000.0, 50000.0, &gs, &config, area, max_lc),
            None
        );
    }
}
