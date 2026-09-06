use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::graph::{GraphState, MenuItem, ModeBanner, menu_item_from_label};
use crate::settings::Settings;

/// Actions the graph engine can request from its host. Stateful actions
/// (motion, zoom, menu navigation) are consumed by [`apply_action`]; the rest
/// are host-level and returned for dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphAction {
    Quit,
    OpenFile(String),
    ToggleHelp,
    ToggleSearch,
    ToggleMinimap,
    ToggleLegend,
    ToggleGrid,
    ToggleStatus,
    Refresh,
    ReloadConfig,
    TogglePreview,
    ToggleLookingGlass,
    MenuAction(MenuItem),
    ConnectionEvent {
        source_id: String,
        target_title: String,
        create: bool,
    },
    ClearFocus,

    // Stateful actions (consumed by `apply_action`)
    PanUp,
    PanDown,
    PanLeft,
    PanRight,
    ZoomIn,
    ZoomOut,
    AutoFit,
    OpenSelected,
    MenuUp,
    MenuDown,
    MenuSelect,
    MenuClose,
}

use GraphAction::*;

/// Apply one action to the graph state. Stateful variants are consumed and
/// return `None`; host-level variants return `Some(action)` unchanged (or, for
/// `OpenSelected`, resolve to `OpenFile(id)`). `MenuSelect` closes the menu and
/// returns the picked `MenuAction`; `MenuClose` closes an open menu or falls
/// back to `Quit`.
pub fn apply_action(
    state: &Arc<RwLock<GraphState>>,
    action: GraphAction,
    settings: &Settings,
) -> Option<GraphAction> {
    let mut guard = state.write();
    match action {
        action @ (Quit
        | OpenFile(_)
        | ToggleHelp
        | ToggleSearch
        | ToggleMinimap
        | ToggleLegend
        | ToggleGrid
        | ToggleStatus
        | Refresh
        | ReloadConfig
        | TogglePreview
        | ToggleLookingGlass
        | MenuAction(_)
        | ConnectionEvent { .. }
        | ClearFocus) => Some(action),
        PanUp => {
            select_in_direction(&mut guard, 0.0, 1.0);
            None
        }
        PanDown => {
            select_in_direction(&mut guard, 0.0, -1.0);
            None
        }
        PanLeft => {
            select_in_direction(&mut guard, -1.0, 0.0);
            None
        }
        PanRight => {
            select_in_direction(&mut guard, 1.0, 0.0);
            None
        }
        ZoomIn => {
            guard.viewport.zoom_in(settings.interaction.zoom_factor);
            None
        }
        ZoomOut => {
            guard.viewport.zoom_out(settings.interaction.zoom_factor);
            None
        }
        AutoFit => {
            let padding = auto_fit_padding(settings);
            let vp = guard
                .viewport
                .clone()
                .auto_fit_from_graph(guard.simulation.get_graph(), padding);
            guard.viewport = vp;
            None
        }
        OpenSelected => {
            if let Some(idx) = guard.selection.primary
                && let Some(node) = guard.simulation.get_graph().node_weight(idx)
            {
                return Some(GraphAction::OpenFile(node.data.id.clone()));
            }
            None
        }
        MenuUp => {
            if let Some(menu) = &mut guard.context_menu {
                menu.move_up();
            }
            None
        }
        MenuDown => {
            if let Some(menu) = &mut guard.context_menu {
                menu.move_down();
            }
            None
        }
        MenuSelect => {
            let dispatch = guard.context_menu.as_ref().and_then(|menu| {
                menu.items
                    .get(menu.selected)
                    .and_then(|spec| menu_item_from_label(spec.label))
            });
            if dispatch.is_some() {
                guard.context_menu = None;
            }
            dispatch.map(GraphAction::MenuAction)
        }
        MenuClose => {
            if guard.context_menu.take().is_some() {
                None
            } else {
                Some(GraphAction::Quit)
            }
        }
    }
}

fn auto_fit_padding(settings: &Settings) -> f64 {
    let p = settings.interaction.auto_fit_padding;
    if p.is_finite() && p > 0.0 { p } else { 1.4 }
}

/// Key bindings for graph actions. `Default` reproduces the standalone
/// binary's historical hardcoded map.
#[derive(Debug, Clone)]
pub struct GraphKeymap {
    pub bindings: Vec<(KeyEvent, GraphAction)>,
}

fn binding(code: KeyCode, ctrl: bool, action: GraphAction) -> (KeyEvent, GraphAction) {
    let mut modifiers = if ctrl {
        crossterm::event::KeyModifiers::CONTROL
    } else {
        crossterm::event::KeyModifiers::NONE
    };
    if let KeyCode::Char(c) = code
        && c.is_uppercase()
    {
        modifiers |= crossterm::event::KeyModifiers::SHIFT;
    }
    (KeyEvent::new(code, modifiers), action)
}

impl Default for GraphKeymap {
    fn default() -> Self {
        Self {
            bindings: vec![
                binding(KeyCode::Esc, false, Quit),
                binding(KeyCode::Char('q'), false, Quit),
                binding(KeyCode::Up, false, PanUp),
                binding(KeyCode::Char('k'), false, PanUp),
                binding(KeyCode::Down, false, PanDown),
                binding(KeyCode::Char('j'), false, PanDown),
                binding(KeyCode::Left, false, PanLeft),
                binding(KeyCode::Char('h'), false, PanLeft),
                binding(KeyCode::Right, false, PanRight),
                binding(KeyCode::Char('l'), false, PanRight),
                binding(KeyCode::Char('+'), false, ZoomIn),
                binding(KeyCode::Char('='), false, ZoomIn),
                binding(KeyCode::Char('j'), true, ZoomIn),
                binding(KeyCode::Char('-'), false, ZoomOut),
                binding(KeyCode::Char('k'), true, ZoomOut),
                binding(KeyCode::Enter, false, OpenSelected),
                binding(KeyCode::Char('a'), false, AutoFit),
                binding(KeyCode::Char('r'), true, ReloadConfig),
                binding(KeyCode::Char('r'), false, Refresh),
                binding(KeyCode::Char('f'), false, ToggleSearch),
                binding(KeyCode::Char('?'), false, ToggleHelp),
                binding(KeyCode::Char('m'), false, ToggleMinimap),
                binding(KeyCode::Char('l'), true, ToggleLegend),
                binding(KeyCode::Char('g'), true, ToggleGrid),
                binding(KeyCode::Char('s'), true, ToggleStatus),
            ],
        }
    }
}

/// Key handler dispatching through a caller-supplied [`GraphKeymap`]; the
/// context-menu and Escape fallthrough branches run before the keymap lookup.
pub fn handle_graph_keys(
    state: &Arc<RwLock<GraphState>>,
    key: KeyEvent,
    settings: &Settings,
    keymap: &GraphKeymap,
) -> Option<GraphAction> {
    // Context menu open: keys drive the menu exclusively.
    {
        let mut guard = state.write();
        if let Some(menu) = guard.context_menu.as_mut() {
            let mut dispatch: Option<GraphAction> = None;
            let mut close = false;

            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => close = true,
                KeyCode::Up | KeyCode::Char('k') => menu.move_up(),
                KeyCode::Down | KeyCode::Char('j') => menu.move_down(),
                KeyCode::Enter => {
                    if let Some(spec) = menu.items.get(menu.selected)
                        && let Some(item) = menu_item_from_label(spec.label)
                    {
                        dispatch = Some(GraphAction::MenuAction(item));
                        close = true;
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(idx) = menu.find_shortcut(c)
                        && let Some(spec) = menu.items.get(idx)
                        && let Some(item) = menu_item_from_label(spec.label)
                    {
                        dispatch = Some(GraphAction::MenuAction(item));
                        close = true;
                    }
                }
                _ => {}
            }

            if close {
                guard.context_menu = None;
            }
            return dispatch;
        }

        // Escape: cancel connection modes, clear the focus filter, or clear
        // multi-select — before falling through to quit.
        if key.code == KeyCode::Esc {
            if guard.connection_source.is_some() || guard.deleting_connection_source.is_some() {
                guard.connection_source = None;
                guard.deleting_connection_source = None;
                guard.mode_banner = None;
                return None;
            }
            if matches!(
                guard.mode_banner,
                Some(ModeBanner::LocalGraph | ModeBanner::GroupedGraph)
            ) {
                return Some(GraphAction::ClearFocus);
            }
            if !guard.selection.extra.is_empty() {
                guard.selection.clear_set();
                guard.mode_banner = None;
                return None;
            }
        }
    }

    let action = keymap
        .bindings
        .iter()
        .find(|(k, _)| k.code == key.code && k.modifiers == key.modifiers)
        .map(|(_, action)| action.clone())?;

    apply_action(state, action, settings)
}

#[derive(Default)]
pub struct GraphMouseState {
    pub drag_origin: Option<(u16, u16)>,
    pub is_panning: bool,
    pub last_click_time: Option<Instant>,
    pub last_clicked_node: Option<fdg_sim::petgraph::graph::NodeIndex>,
    pub is_minimap_dragging: bool,
    pub middle_drag_origin: Option<(u16, u16)>,
    pub is_middle_panning: bool,
}

pub fn handle_graph_mouse(
    state: &Arc<RwLock<GraphState>>,
    mouse_event: MouseEvent,
    area: Rect,
    mouse_state: &mut GraphMouseState,
    settings: &Settings,
    show_status_bar: bool,
) -> Option<GraphAction> {
    let canvas = crate::render::canvas_area(area, show_status_bar);
    let minimap_area = if settings.visual.show_minimap {
        Some(crate::render::compute_minimap_area(canvas, settings))
    } else {
        None
    };

    let in_minimap = minimap_area.is_some_and(|ma| {
        mouse_event.column >= ma.x
            && mouse_event.column < ma.x + ma.width
            && mouse_event.row >= ma.y
            && mouse_event.row < ma.y + ma.height
    });

    let inside_area = mouse_event.column >= canvas.x
        && mouse_event.column < canvas.x + canvas.width
        && mouse_event.row >= canvas.y
        && mouse_event.row < canvas.y + canvas.height;

    match mouse_event.kind {
        MouseEventKind::ScrollUp => {
            if !inside_area {
                return None;
            }
            let mut guard = state.write();
            guard.viewport.zoom_in(settings.interaction.zoom_factor);
        }
        MouseEventKind::ScrollDown => {
            if !inside_area {
                return None;
            }
            let mut guard = state.write();
            guard.viewport.zoom_out(settings.interaction.zoom_factor);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if !inside_area {
                return None;
            }
            // Context menu: click inside activates a row, click outside dismisses.
            {
                let mut guard = state.write();
                if let Some(menu) = guard.context_menu.take() {
                    let rect = menu.rect(canvas);
                    if let Some(idx) = menu.row_at(rect, mouse_event.column, mouse_event.row)
                        && let Some(spec) = menu.items.get(idx)
                        && let Some(item) = menu_item_from_label(spec.label)
                    {
                        return Some(GraphAction::MenuAction(item));
                    }
                    return None;
                }
            }
            // Connection mode: clicking a target completes (or cancels) the link.
            {
                let mut conn_action: Option<GraphAction> = None;
                let mut in_conn_mode = false;
                {
                    let guard = state.read();
                    let src_idx = guard.connection_source.or(guard.deleting_connection_source);
                    if src_idx.is_some() {
                        in_conn_mode = true;
                        let create = guard.connection_source.is_some();
                        let source_id = src_idx
                            .and_then(|idx| guard.simulation.get_graph().node_weight(idx))
                            .map(|n| n.data.id.clone());
                        let (wx, wy) = guard.viewport.screen_to_world(
                            mouse_event.column,
                            mouse_event.row,
                            canvas,
                        );
                        let max_lc = guard.render_cache.lock().max_link_count;
                        let target_idx = guard
                            .viewport
                            .hit_test(wx, wy, &guard, settings, canvas, max_lc);
                        if let (Some(src), Some(source_id), Some(tidx)) =
                            (src_idx, source_id, target_idx)
                            && src != tidx
                            && let Some(target_title) = guard
                                .simulation
                                .get_graph()
                                .node_weight(tidx)
                                .map(|n| n.data.title.clone())
                        {
                            conn_action = Some(GraphAction::ConnectionEvent {
                                source_id,
                                target_title,
                                create,
                            });
                        }
                    }
                }
                if in_conn_mode {
                    let mut g = state.write();
                    g.connection_source = None;
                    g.deleting_connection_source = None;
                    g.mode_banner = None;
                    if let Some(a) = conn_action {
                        return Some(a);
                    }
                    return None;
                }
            }
            if in_minimap {
                if let Some(ma) = minimap_area {
                    let world = minimap_screen_to_world(
                        mouse_event.column,
                        mouse_event.row,
                        ma,
                        &state.read(),
                    );
                    let mut guard = state.write();
                    guard.viewport.set_center(world.0, world.1);
                    mouse_state.is_minimap_dragging = true;
                    mouse_state.drag_origin = Some((mouse_event.column, mouse_event.row));
                }
            } else {
                let (wx, wy) = {
                    let guard = state.read();
                    guard
                        .viewport
                        .screen_to_world(mouse_event.column, mouse_event.row, canvas)
                };

                let hit = {
                    let guard = state.read();
                    let max_lc = guard.render_cache.lock().max_link_count;
                    guard
                        .viewport
                        .hit_test(wx, wy, &guard, settings, canvas, max_lc)
                };

                let is_double_click = mouse_state.last_click_time.is_some_and(|t| {
                    t.elapsed().as_millis() < settings.interaction.double_click_ms as u128
                });

                if let Some(node_idx) = hit {
                    let mut guard = state.write();
                    guard.selection.select_only(node_idx);
                    guard.dragging_node = Some(node_idx);
                    mouse_state.drag_origin = Some((mouse_event.column, mouse_event.row));
                    mouse_state.is_panning = false;
                    mouse_state.last_clicked_node = Some(node_idx);

                    if is_double_click
                        && let Some(node) = guard.simulation.get_graph().node_weight(node_idx)
                    {
                        mouse_state.last_click_time = Some(Instant::now());
                        return Some(GraphAction::OpenFile(node.data.id.clone()));
                    }
                } else {
                    let mut guard = state.write();
                    guard.selection.clear();
                    guard.dragging_node = None;
                    mouse_state.drag_origin = Some((mouse_event.column, mouse_event.row));
                    mouse_state.is_panning = true;
                    mouse_state.last_clicked_node = None;
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let (orig_col, orig_row) = mouse_state.drag_origin?;

            if mouse_state.is_minimap_dragging {
                if let Some(ma) = minimap_area {
                    let world = minimap_screen_to_world(
                        mouse_event.column,
                        mouse_event.row,
                        ma,
                        &state.read(),
                    );
                    let mut guard = state.write();
                    guard.viewport.set_center(world.0, world.1);
                    mouse_state.drag_origin = Some((mouse_event.column, mouse_event.row));
                }
            } else if mouse_state.is_panning {
                let mut guard = state.write();
                let aspect = canvas.width as f64 / canvas.height.max(1) as f64;
                let [xl, xr] = guard.viewport.x_bounds(aspect);
                let [yb, yt] = guard.viewport.y_bounds(aspect);
                let world_per_col = ((xr - xl) / canvas.width.max(1) as f64).abs();
                let world_per_row = ((yt - yb) / canvas.height.max(1) as f64).abs();
                let world_dx = -world_per_col
                    * (mouse_event.column as f64 - orig_col as f64)
                    * settings.interaction.drag_sensitivity;
                let world_dy = world_per_row
                    * (mouse_event.row as f64 - orig_row as f64)
                    * settings.interaction.drag_sensitivity;
                guard.viewport.pan_by(world_dx, world_dy);
                mouse_state.drag_origin = Some((mouse_event.column, mouse_event.row));
            } else {
                let (wx, wy) = {
                    let guard = state.read();
                    guard
                        .viewport
                        .screen_to_world(mouse_event.column, mouse_event.row, canvas)
                };

                let mut guard = state.write();
                if let Some(node_idx) = guard.dragging_node {
                    let graph = guard.simulation.get_graph_mut();
                    if let Some(node) = graph.node_weight_mut(node_idx) {
                        node.location.x = wx as f32;
                        node.location.y = wy as f32;
                        node.velocity = fdg_sim::glam::Vec3::ZERO;
                    }
                    if guard.physics_worker_active {
                        guard.drag_target = Some((wx as f32, wy as f32));
                        guard.reheat(0.4);
                    } else {
                        guard.alpha = 0.0;
                        guard.is_settled = true;
                        let bounds =
                            crate::render::compute_graph_bounds(guard.simulation.get_graph());
                        guard.graph_bounds = bounds;
                        guard.render_cache.lock().minimap_dirty = true;
                    }
                }
                mouse_state.drag_origin = Some((mouse_event.column, mouse_event.row));
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            {
                let mut guard = state.write();
                guard.dragging_node = None;
                guard.drag_target = None;
            }
            mouse_state.drag_origin = None;
            mouse_state.is_panning = false;
            mouse_state.is_minimap_dragging = false;
            mouse_state.last_click_time = Some(Instant::now());
        }

        MouseEventKind::Down(MouseButton::Middle) => {
            if !inside_area {
                return None;
            }
            mouse_state.middle_drag_origin = Some((mouse_event.column, mouse_event.row));
            mouse_state.is_middle_panning = true;
        }
        MouseEventKind::Drag(MouseButton::Middle) => {
            if mouse_state.is_middle_panning {
                let (orig_col, orig_row) = mouse_state.middle_drag_origin?;
                let mut guard = state.write();
                let aspect = canvas.width as f64 / canvas.height.max(1) as f64;
                let [xl, xr] = guard.viewport.x_bounds(aspect);
                let [yb, yt] = guard.viewport.y_bounds(aspect);
                let world_per_col = ((xr - xl) / canvas.width.max(1) as f64).abs();
                let world_per_row = ((yt - yb) / canvas.height.max(1) as f64).abs();
                let world_dx = -world_per_col
                    * (mouse_event.column as f64 - orig_col as f64)
                    * settings.interaction.drag_sensitivity;
                let world_dy = world_per_row
                    * (mouse_event.row as f64 - orig_row as f64)
                    * settings.interaction.drag_sensitivity;
                guard.viewport.pan_by(world_dx, world_dy);
                mouse_state.middle_drag_origin = Some((mouse_event.column, mouse_event.row));
            }
        }
        MouseEventKind::Up(MouseButton::Middle) => {
            mouse_state.middle_drag_origin = None;
            mouse_state.is_middle_panning = false;
        }

        MouseEventKind::Down(MouseButton::Right) => {
            if !inside_area {
                return None;
            }
            let (wx, wy) = {
                let guard = state.read();
                guard
                    .viewport
                    .screen_to_world(mouse_event.column, mouse_event.row, canvas)
            };
            let mut guard = state.write();
            guard.right_down_pos = Some((mouse_event.column, mouse_event.row));
            guard.marquee.on_down(wx, wy);
        }
        MouseEventKind::Drag(MouseButton::Right) => {
            let start = {
                let guard = state.read();
                guard.right_down_pos
            };
            let (sx, sy) = start?;
            let dragging = {
                let guard = state.read();
                guard
                    .marquee
                    .is_dragging_screen(mouse_event.column, mouse_event.row, sx, sy)
            };
            if dragging {
                let (wx, wy) = {
                    let guard = state.read();
                    guard
                        .viewport
                        .screen_to_world(mouse_event.column, mouse_event.row, canvas)
                };
                let mut guard = state.write();
                if guard.mode_banner.is_none() {
                    guard.mode_banner = Some(ModeBanner::BoxSelect);
                }
                guard.marquee.on_drag(wx, wy);
                guard.context_menu = None;
            }
        }
        MouseEventKind::Up(MouseButton::Right) => {
            let start_screen = {
                let guard = state.read();
                guard.right_down_pos
            };
            let Some((sx, sy)) = start_screen else {
                let mut g = state.write();
                g.right_down_pos = None;
                g.marquee.clear();
                return None;
            };

            let dragging = {
                let guard = state.read();
                guard
                    .marquee
                    .is_dragging_screen(mouse_event.column, mouse_event.row, sx, sy)
            };

            let mut guard = state.write();
            guard.right_down_pos = None;
            let commit_rect = guard.marquee.commit_rect();
            guard.marquee.clear();

            if !dragging {
                // Click → context menu.
                if guard.selection.extra.is_empty() {
                    let (wx, wy) =
                        guard
                            .viewport
                            .screen_to_world(mouse_event.column, mouse_event.row, canvas);
                    let max_lc = guard.render_cache.lock().max_link_count;
                    if let Some(idx) = guard
                        .viewport
                        .hit_test(wx, wy, &guard, settings, canvas, max_lc)
                    {
                        guard.selection.select_only(idx);
                    }
                    guard.open_context_menu(mouse_event.column, mouse_event.row, (wx, wy));
                } else {
                    guard.open_context_menu(mouse_event.column, mouse_event.row, (0.0, 0.0));
                }
            } else if let Some((min_x, min_y, max_x, max_y)) = commit_rect {
                // Box-select commit: collect enclosed nodes.
                let mut enclosed: Vec<fdg_sim::petgraph::graph::NodeIndex> = Vec::new();
                {
                    let graph = guard.simulation.get_graph();
                    for idx in graph.node_indices() {
                        let node = &graph[idx];
                        let nx = node.location.x as f64;
                        let ny = node.location.y as f64;
                        if nx >= min_x && nx <= max_x && ny >= min_y && ny <= max_y {
                            enclosed.push(idx);
                        }
                    }
                }
                let primary = enclosed.first().copied();
                guard
                    .selection
                    .replace_set(enclosed.into_iter().collect(), primary);
                if guard.mode_banner == Some(ModeBanner::BoxSelect) {
                    guard.mode_banner = None;
                }
            }
        }
        _ => {}
    }

    None
}

fn select_in_direction(guard: &mut GraphState, dx: f64, dy: f64) {
    if guard.selection.primary.is_none() {
        let nearest = guard.viewport.nearest_to_center(guard);
        if let Some(idx) = nearest {
            guard.selection.select_only(idx);
            let graph = guard.simulation.get_graph();
            let node = &graph[idx];
            guard
                .viewport
                .center_on_node(node.location.x, node.location.y);
        }
        return;
    }

    let Some(idx) = guard.selection.primary else {
        return;
    };
    let (ox, oy) = {
        let graph = guard.simulation.get_graph();
        let node = &graph[idx];
        (node.location.x as f64, node.location.y as f64)
    };

    if let Some(next) =
        guard
            .viewport
            .nearest_in_direction(guard, ox, oy, dx, dy, guard.selection.primary)
    {
        guard.selection.select_only(next);
        let graph = guard.simulation.get_graph();
        let node = &graph[next];
        guard
            .viewport
            .center_on_node(node.location.x, node.location.y);
    }
}

fn minimap_screen_to_world(
    col: u16,
    row: u16,
    minimap_area: Rect,
    state: &GraphState,
) -> (f64, f64) {
    let (wx_min, wx_max, wy_min, wy_max) = state.graph_bounds;
    let inner_x = minimap_area.x + 1;
    let inner_y = minimap_area.y + 1;
    let inner_w = minimap_area.width.saturating_sub(2);
    let inner_h = minimap_area.height.saturating_sub(2);

    if inner_w == 0 || inner_h == 0 {
        return (0.0, 0.0);
    }

    let rel_x = (col as f64 - inner_x as f64) / inner_w as f64;
    let rel_y = 1.0 - (row as f64 - inner_y as f64) / inner_h as f64;

    let wx = wx_min + rel_x * (wx_max - wx_min);
    let wy = wy_min + rel_y * (wy_max - wy_min);
    (wx, wy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::NodeSpec;
    use crossterm::event::KeyModifiers;
    #[test]
    fn test_keymap_injection_overrides_bindings() {
        let state = setup_mock_graph_state(&[("a", &[]), ("b", &["a"])]);
        let settings = Settings::default();
        let mut keymap = GraphKeymap::default();
        // Unbind `q` and `+`; bind `z` to Quit instead.
        keymap
            .bindings
            .retain(|(k, _)| k.code != KeyCode::Char('q') && k.code != KeyCode::Char('+'));
        keymap
            .bindings
            .push(binding(KeyCode::Char('z'), false, Quit));

        // Injected binding dispatches (Quit is host-level: returned as-is).
        let z = handle_graph_keys(
            &state,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
            &settings,
            &keymap,
        );
        assert_eq!(z, Some(Quit));

        // Removed defaults no longer fire.
        let q = handle_graph_keys(
            &state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &settings,
            &keymap,
        );
        assert_eq!(q, None);
        let plus = handle_graph_keys(
            &state,
            KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE),
            &settings,
            &keymap,
        );
        assert_eq!(plus, None);
    }
    use parking_lot::RwLock;

    fn setup_mock_graph_state(nodes: &[(&str, &[&str])]) -> Arc<RwLock<GraphState>> {
        let specs: Vec<NodeSpec> = nodes
            .iter()
            .map(|(id, links)| NodeSpec {
                id: id.to_string(),
                title: id.to_string(),
                tags: vec![],
                folder: String::new(),
                links: links.iter().map(|s| s.to_string()).collect(),
            })
            .collect();
        let mut settings = Settings::default();
        settings.visual.show_minimap = true;
        settings.filter.show_orphan = true;
        let gs = GraphState::from_specs(&specs, &settings).unwrap();
        Arc::new(RwLock::new(gs))
    }

    #[test]
    fn test_middle_mouse_pan_lifecycle() {
        let state = setup_mock_graph_state(&[("a", &["b"]), ("b", &["a"])]);
        let mut mouse_state = GraphMouseState::default();
        let settings = Settings::default();
        let area = Rect::new(0, 0, 100, 50);

        // 1. Middle down inside canvas
        let down_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Middle),
            column: 50,
            row: 25,
            modifiers: KeyModifiers::NONE,
        };
        let res = handle_graph_mouse(&state, down_event, area, &mut mouse_state, &settings, false);
        assert!(res.is_none());
        assert!(mouse_state.is_middle_panning);
        assert_eq!(mouse_state.middle_drag_origin, Some((50, 25)));

        // 2. Middle drag
        let drag_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Middle),
            column: 60,
            row: 30,
            modifiers: KeyModifiers::NONE,
        };
        let res = handle_graph_mouse(&state, drag_event, area, &mut mouse_state, &settings, false);
        assert!(res.is_none());
        assert_eq!(mouse_state.middle_drag_origin, Some((60, 30)));

        // 3. Middle up
        let up_event = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Middle),
            column: 60,
            row: 30,
            modifiers: KeyModifiers::NONE,
        };
        let res = handle_graph_mouse(&state, up_event, area, &mut mouse_state, &settings, false);
        assert!(res.is_none());
        assert!(!mouse_state.is_middle_panning);
        assert_eq!(mouse_state.middle_drag_origin, None);
    }

    #[test]
    fn test_middle_mouse_pan_over_node_does_not_drag_node() {
        let state = setup_mock_graph_state(&[("a", &["b"]), ("b", &["a"])]);
        let mut mouse_state = GraphMouseState::default();
        let settings = Settings::default();
        let area = Rect::new(0, 0, 100, 50);

        let canvas = crate::render::canvas_area(area, false);

        // Locate screen position of node 0
        let (col, row) = {
            let mut g = state.write();
            g.viewport = crate::viewport::Viewport::default();
            let graph = g.simulation.get_graph_mut();
            let node_idx = fdg_sim::petgraph::graph::NodeIndex::new(0);
            let node = &mut graph[node_idx];
            node.location.x = 0.0;
            node.location.y = 0.0;
            let (sc, sr) = g.viewport.world_to_screen(0.0, 0.0, canvas);
            (sc.round() as u16, sr.round() as u16)
        };

        let down_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Middle),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        };
        handle_graph_mouse(&state, down_event, area, &mut mouse_state, &settings, false);

        assert!(mouse_state.is_middle_panning);
        {
            let g = state.read();
            assert_eq!(g.dragging_node, None);
            assert_eq!(g.selection.primary, None);
        }

        let drag_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Middle),
            column: col.saturating_add(5),
            row: row.saturating_add(5),
            modifiers: KeyModifiers::NONE,
        };
        handle_graph_mouse(&state, drag_event, area, &mut mouse_state, &settings, false);

        {
            let g = state.read();
            assert_eq!(g.dragging_node, None);
        }
    }

    #[test]
    fn test_middle_mouse_pan_over_minimap() {
        let state = setup_mock_graph_state(&[("a", &["b"]), ("b", &["a"])]);
        let mut mouse_state = GraphMouseState::default();
        let mut settings = Settings::default();
        settings.visual.show_minimap = true;
        let area = Rect::new(0, 0, 100, 50);

        let canvas = crate::render::canvas_area(area, false);
        let minimap = crate::render::compute_minimap_area(canvas, &settings);

        let down_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Middle),
            column: minimap.x + 1,
            row: minimap.y + 1,
            modifiers: KeyModifiers::NONE,
        };
        handle_graph_mouse(&state, down_event, area, &mut mouse_state, &settings, false);

        assert!(mouse_state.is_middle_panning);
        assert!(!mouse_state.is_minimap_dragging);

        let drag_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Middle),
            column: minimap.x + 4,
            row: minimap.y + 3,
            modifiers: KeyModifiers::NONE,
        };
        handle_graph_mouse(&state, drag_event, area, &mut mouse_state, &settings, false);

        assert!(!mouse_state.is_minimap_dragging);
    }

    #[test]
    fn test_middle_mouse_outside_area_ignored() {
        let state = setup_mock_graph_state(&[("a", &["b"])]);
        let mut mouse_state = GraphMouseState::default();
        let settings = Settings::default();
        let area = Rect::new(10, 10, 20, 20);

        let down_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Middle),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        let res = handle_graph_mouse(&state, down_event, area, &mut mouse_state, &settings, false);
        assert!(res.is_none());
        assert!(!mouse_state.is_middle_panning);
        assert_eq!(mouse_state.middle_drag_origin, None);
    }

    #[test]
    fn test_context_menu_click_dispatches_item() {
        let state = setup_mock_graph_state(&[("a", &["b"]), ("b", &["a"])]);
        let mut mouse_state = GraphMouseState::default();
        let settings = Settings::default();
        let area = Rect::new(0, 0, 100, 50);

        {
            let mut g = state.write();
            g.selection
                .select_only(fdg_sim::petgraph::graph::NodeIndex::new(0));
            g.open_context_menu(30, 20, (0.0, 0.0));
            assert!(g.context_menu.is_some());
        }

        // Click the first menu row.
        let (menu_x, menu_y) = {
            let g = state.read();
            let rect = g.context_menu.as_ref().unwrap().rect(area);
            (rect.x + 1, rect.y)
        };
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: menu_x,
            row: menu_y,
            modifiers: KeyModifiers::NONE,
        };
        let action = handle_graph_mouse(&state, click, area, &mut mouse_state, &settings, false);
        assert!(matches!(action, Some(GraphAction::MenuAction(_))));
        assert!(
            state.read().context_menu.is_none(),
            "menu closed after click"
        );

        // Clicking outside an open menu dismisses it without dispatch.
        {
            let mut g = state.write();
            g.open_context_menu(50, 25, (0.0, 0.0));
        }
        let outside = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };
        let action = handle_graph_mouse(&state, outside, area, &mut mouse_state, &settings, false);
        assert!(action.is_none());
        assert!(state.read().context_menu.is_none());
    }
}
