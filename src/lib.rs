#[allow(dead_code)]
mod document;
pub mod graph;
pub mod linker;
pub mod settings;
pub mod theme;
mod util;

// Placeholder modules that will be ported in next steps
pub mod input;
pub mod physics;
pub mod render;
pub mod viewport;
pub use input::{
    GraphAction, GraphKeymap, GraphMouseState, MouseAction, apply_action, handle_graph_keys,
    handle_graph_mouse, handle_graph_mouse_by_id, handle_graph_mouse_by_id_with_flags,
};
pub use physics::{simulation_step, start_physics};
pub use render::{
    FeatureFlags, RenderCache, canvas_area, compute_graph_bounds, compute_minimap_area,
    draw_graph_view,
};

pub use settings::{
    Background, CanvasMarker, EdgeColorMode, LabelMode, LegendPosition, NodeColorMode, NodeFill,
    NodeScale, NodeShape, NodeSizeMode, PhysicsTickRate, Settings,
};
pub mod wikilink;
pub use graph::{
    ContextMenu, GraphNodeData, GraphState, MarqueeState, MenuItem, MenuItemSpec, ModeBanner,
    NodeSpec, Selection, apply_connection_change, build_graph, build_graph_resolved,
    create_simulation, menu_item_from_label, menu_specs, search_nodes,
};
pub use linker::{FileData, resolve_links, scan_markdown_files};
pub use theme::ThemeColors;
pub use wikilink::{add_wikilink, remove_wikilink};

/// Alias for [`resolve_links`]: resolves wikilink targets to relative paths.
pub fn resolve_wikilinks(
    files: &[linker::FileData],
    exclude_tags: &[String],
) -> std::collections::HashMap<String, Vec<String>> {
    linker::resolve_links(files, exclude_tags)
}

/// Returns an empty map — markdown-link resolution requires the upstream
/// linker feature; callers should use [`resolve_links`] instead.
pub fn resolve_markdown_links(
    files: &[linker::FileData],
    _exclude_tags: &[String],
) -> std::collections::HashMap<String, Vec<String>> {
    files
        .iter()
        .map(|f| (f.relative_path.clone(), Vec::new()))
        .collect()
}
