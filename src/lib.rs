pub mod settings;
pub mod theme;
pub mod linker;
pub mod graph;

// Placeholder modules that will be ported in next steps
pub mod physics;
pub mod viewport;
pub mod render;
pub mod input;

pub use settings::{Settings, Background, NodeColorMode, EdgeColorMode, LabelMode, NodeSizeMode, CanvasMarker, NodeShape, LegendPosition, PhysicsTickRate};
pub mod wikilink;
pub use theme::ThemeColors;
pub use wikilink::{add_wikilink, remove_wikilink};
pub use linker::{FileData, scan_markdown_files, resolve_links};
pub use graph::{
    Selection, MenuItemSpec, ContextMenu, MarqueeState, NodeSpec, GraphNodeData,
    menu_specs, menu_item_from_label, MenuItem, ModeBanner, GraphState,
    build_graph, create_simulation, search_nodes, apply_connection_change
};
