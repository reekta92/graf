pub mod graph;
pub mod linker;
pub mod settings;
pub mod theme;

// Placeholder modules that will be ported in next steps
pub mod input;
pub mod physics;
pub mod render;
pub mod viewport;
pub use input::{
    GraphAction, GraphKeymap, GraphMouseState, apply_action, handle_graph_keys, handle_graph_mouse,
};
pub use physics::{simulation_step, start_physics};
pub use render::{
    FeatureFlags, RenderCache, canvas_area, compute_graph_bounds, compute_minimap_area,
    draw_graph_view,
};

pub use settings::{
    Background, CanvasMarker, EdgeColorMode, LabelMode, LegendPosition, NodeColorMode, NodeShape,
    NodeSizeMode, PhysicsTickRate, Settings,
};
pub mod wikilink;
pub use graph::{
    ContextMenu, GraphNodeData, GraphState, MarqueeState, MenuItem, MenuItemSpec, ModeBanner,
    NodeSpec, Selection, apply_connection_change, build_graph, create_simulation,
    menu_item_from_label, menu_specs, search_nodes,
};
pub use linker::{FileData, resolve_links, scan_markdown_files};
pub use theme::ThemeColors;
pub use wikilink::{add_wikilink, remove_wikilink};
