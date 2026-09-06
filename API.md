# graf — Library API

`graf` is both a standalone TUI application and an embeddable library. This document covers using it as a library crate (e.g. embedding a graph view inside another ratatui application like [clin-rs](https://github.com/reekta92/clin-rs)).

- - -

## Adding the dependency

```toml
# Cargo.toml
[dependencies]
graf = { package = "graf-rs", git = "https://github.com/reekta92/graf", tag = "v1.0.0" }

# graf re-exports symbols from these crates; you will likely need them directly
fdg-sim = "0.9"
parking_lot = "0.12"
ratatui = "0.30"
```

- - -

## Core concepts

graf's library surface is organized into four layers:

| Layer | Purpose | Key types |
|-------|---------|-----------|
| **Data** | Describe your nodes | `NodeSpec`, `FileData`, `scan_markdown_files`, `resolve_links` |
| **Graph** | Build and query the force graph | `GraphState`, `build_graph`, `create_simulation`, `search_nodes` |
| **Physics** | Run the force-directed layout | `start_physics`, `simulation_step` |
| **Rendering** | Draw the graph into a ratatui frame | `draw_graph_view`, `FeatureFlags`, `RenderCache` |
| **Input** | Handle keyboard/mouse events | `handle_graph_keys`, `handle_graph_mouse`, `apply_action`, `GraphAction` |
| **Config** | Settings, themes, enums | `Settings`, `ThemeColors`, `Background`, `NodeColorMode`, … |

- - -

## Quick start

Minimal integration: build a graph from node specs, start physics, render each frame, and forward input events.

```rust
use std::sync::Arc;
use parking_lot::RwLock;
use graf::{
    GraphState, NodeSpec, Settings, ThemeColors, FeatureFlags,
    draw_graph_view, start_physics, handle_graph_keys, handle_graph_mouse,
    apply_action, GraphAction, GraphKeymap, GraphMouseState,
};

// 1. Define your nodes
let specs = vec![
    NodeSpec {
        id: "note-1".into(),
        title: "First Note".into(),
        tags: vec!["rust".into()],
        folder: "notes".into(),
        links: vec!["Second Note".into()],
    },
    NodeSpec {
        id: "note-2".into(),
        title: "Second Note".into(),
        tags: vec!["rust".into()],
        folder: "notes".into(),
        links: vec![],
    },
];

// 2. Build graph state (graph + simulation + auto-fitted viewport)
let settings = Settings::default();
let graph_state = GraphState::from_specs(&specs, &settings)
    .expect("graph build failed");
let state = Arc::new(RwLock::new(graph_state));

// 3. Start physics on a background thread
//    Returns a kill channel sender; drop it or send () to stop.
//    Returns None for empty graphs or >1000 nodes (static layout used instead).
let kill_tx = start_physics(state.clone(), &settings);

// 4. Render inside your ratatui draw closure
let theme = ThemeColors::resolve(&settings);
let flags = FeatureFlags {
    show_legend: true,
    grid: false,
    show_minimap: true,
    show_status_bar: false,
};

// In your ratatui draw loop:
// terminal.draw(|frame| {
//     let area = frame.area();
//     let guard = state.read();
//     draw_graph_view(frame, area, &guard, &settings, &theme, &flags);
// });

// 5. Forward input events
let keymap = GraphKeymap::default();
let mut mouse_state = GraphMouseState::default();

// Keyboard:
// if let Event::Key(key) = event {
//     if let Some(action) = handle_graph_keys(&state, key, &settings, &keymap) {
//         match action {
//             GraphAction::Quit => break,
//             GraphAction::OpenFile(path) => { /* open in editor */ }
//             other => { /* handle or ignore */ }
//         }
//     }
// }

// Mouse:
// if let Event::Mouse(mouse) = event {
//     if let Some(action) = handle_graph_mouse(
//         &state, mouse, area, &mut mouse_state, &settings, false,
//     ) {
//         // dispatch action same as keyboard
//     }
// }

// 6. Cleanup: drop kill_tx or send () to stop the physics thread
if let Some(tx) = kill_tx {
    let _ = tx.send(());
}
```

- - -

## Data layer

### `NodeSpec`

The input type for graph construction. Each node needs:

```rust
pub struct NodeSpec {
    pub id: String,        // unique identifier
    pub title: String,     // display name (used for link resolution)
    pub tags: Vec<String>, // used for coloring in "tag" color mode
    pub folder: String,    // used for coloring in "folder" color mode
    pub links: Vec<String>,// titles of linked nodes (resolved case-insensitively)
}
```

If your data source is a directory of markdown files, use the built-in scanner:

### `scan_markdown_files`

```rust
use std::path::Path;
use graf::{scan_markdown_files, FileData};

let files: Vec<FileData> = scan_markdown_files(
    Path::new("./my-wiki"),
    &["archive/*".into(), "*.bak".into()], // exclude patterns
    500,                                    // max nodes (0 = unlimited)
);
```

`FileData` contains `relative_path`, `title`, `tags`, and `wikilinks` parsed from each markdown file. Convert to `NodeSpec` for graph building:

```rust
let specs: Vec<NodeSpec> = files.iter().map(|f| NodeSpec {
    id: f.relative_path.clone(),
    title: f.title.clone(),
    tags: f.tags.clone(),
    folder: std::path::Path::new(&f.relative_path)
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default(),
    links: f.wikilinks.clone(),
}).collect();
```

### `resolve_links`

Returns a map of file path → resolved link targets (file paths), after case-insensitive title matching and tag filtering:

```rust
use graf::resolve_links;

let link_map = resolve_links(&files, &["draft".into()]);
// link_map: HashMap<String, Vec<String>>
```

### `add_wikilink` / `remove_wikilink`

Pure string transforms for editing wikilinks in markdown content (hosts own file I/O):

```rust
use graf::{add_wikilink, remove_wikilink};

let content = "# My Note\nSome text.";
let updated = add_wikilink(content, "Other Note");
// Appends [[Other Note]] under a ## Links section

let cleaned = remove_wikilink(&updated, "Other Note");
// Removes all [[Other Note]] / [[Other Note|alias]] links
```

- - -

## Graph layer

### `GraphState::from_specs`

The recommended way to build a complete graph state from node specs. Internally calls `build_graph` → `create_simulation` → auto-fits the viewport → computes bounds.

```rust
let state = GraphState::from_specs(&specs, &settings)?;
```

### `GraphState::new`

Lower-level constructor if you need manual control over the simulation and viewport:

```rust
use graf::{build_graph, create_simulation, GraphState};
use graf::viewport::Viewport;

let graph = build_graph(&specs, &settings)?;
let simulation = create_simulation(graph, &settings);
let state = GraphState::new(simulation, Viewport::default());
```

### Key `GraphState` fields

```rust
pub struct GraphState {
    pub simulation: Simulation<GraphNodeData, ()>, // fdg-sim force graph
    pub viewport: Viewport,                        // pan/zoom state
    pub selection: Selection<NodeIndex>,            // selected node(s)
    pub is_settled: bool,                          // physics converged?
    pub mode_banner: Option<ModeBanner>,           // active UI mode
    pub context_menu: Option<ContextMenu>,         // right-click menu
    pub marquee: MarqueeState,                     // box-select state
    // ... plus drag, render cache, mouse state
}
```

### `search_nodes`

Fuzzy search over node titles, tags, and IDs:

```rust
use graf::search_nodes;

let guard = state.read();
let results: Vec<(NodeIndex, String)> = search_nodes(
    &guard.simulation,
    "rust",   // query
    20,       // max results
);
```

### `apply_connection_change`

Add or remove an edge in the live simulation without rebuilding:

```rust
use graf::apply_connection_change;

let mut guard = state.write();
apply_connection_change(&mut guard.simulation, source_idx, target_idx, true /* add */);
```

- - -

## Physics layer

### `start_physics`

Spawns a background thread running the force-directed simulation. Automatically adapts tick rate to node count. For graphs over 1000 nodes, applies a static cluster layout instead.

```rust
let kill_tx: Option<mpsc::Sender<()>> = start_physics(state.clone(), &settings);
```

- Drop the sender or send `()` to stop the thread.
- Returns `None` for empty graphs or when static layout is used (>1000 nodes).
- The thread writes to `state` via `RwLock`; coordinate reads/writes accordingly.

### `simulation_step`

Manual single-step alternative (e.g. for preview thumbnails that don't need a background thread):

```rust
use graf::simulation_step;

let mut guard = state.write();
for _ in 0..10 {
    simulation_step(&mut guard, 0.12);
    if guard.is_settled { break; }
}
```

- - -

## Rendering layer

### `draw_graph_view`

Main render function. Call inside your ratatui `terminal.draw()` closure:

```rust
use graf::{draw_graph_view, FeatureFlags, ThemeColors};

let theme = ThemeColors::resolve(&settings);
let flags = FeatureFlags {
    show_legend: true,
    grid: false,
    show_minimap: true,
    show_status_bar: false,
};

// Inside terminal.draw(|frame| { ... }):
let guard = state.read();
draw_graph_view(frame, area, &guard, &settings, &theme, &flags);
```

This renders the full graph canvas including nodes, edges, labels, minimap, legend, grid, and looking glass — but **not** the status bar or search popup (those are host-owned).

### `FeatureFlags`

Toggle overlay visibility at render time:

```rust
pub struct FeatureFlags {
    pub show_legend: bool,
    pub grid: bool,
    pub show_minimap: bool,
    pub show_status_bar: bool,
}
```

### Helper functions

```rust
use graf::{canvas_area, compute_graph_bounds, compute_minimap_area};

// Compute the drawable canvas area (excludes status bar row if shown)
let canvas = canvas_area(area, show_status_bar);

// Compute world-space bounding box of all nodes
let (min_x, min_y, max_x, max_y) = compute_graph_bounds(simulation.get_graph());

// Compute minimap screen rectangle
let minimap_rect = compute_minimap_area(canvas, &settings);
```

- - -

## Input layer

### `handle_graph_keys`

Processes a keyboard event against the default keymap and graph state. Returns `Some(GraphAction)` for host-level actions, `None` for internally consumed actions (pan, zoom, menu navigation):

```rust
use graf::{handle_graph_keys, GraphKeymap};

let keymap = GraphKeymap::default();
if let Some(action) = handle_graph_keys(&state, key_event, &settings, &keymap) {
    match action {
        GraphAction::Quit => { /* exit graph view */ }
        GraphAction::OpenFile(path) => { /* open file in editor */ }
        GraphAction::ToggleSearch => { /* show/hide search popup */ }
        GraphAction::Refresh => { /* rescan files, rebuild graph */ }
        GraphAction::MenuAction(item) => { /* context menu pick */ }
        GraphAction::ConnectionEvent { source_id, target_title, create } => {
            // Wikilink connection created/deleted
        }
        _ => {}
    }
}
```

### `handle_graph_mouse`

Processes mouse events (click, drag, scroll, double-click):

```rust
use graf::{handle_graph_mouse, GraphMouseState};

let mut mouse_state = GraphMouseState::default();

if let Some(action) = handle_graph_mouse(
    &state,
    mouse_event,
    area,           // the Rect passed to draw_graph_view
    &mut mouse_state,
    &settings,
    false,          // show_status_bar
) {
    // dispatch action
}
```

### `apply_action`

For hosts that resolve their own keybinds (e.g. clin-rs has its own keybind system), you can bypass `handle_graph_keys` and call `apply_action` directly:

```rust
use graf::apply_action;

// Stateful actions (PanUp, ZoomIn, etc.) are consumed and return None.
// Host actions (Quit, OpenFile, etc.) pass through as Some(action).
if let Some(host_action) = apply_action(&state, GraphAction::ZoomIn, &settings) {
    // handle host action
}
```

### `GraphAction`

Full enum of actions the graph engine can produce:

```rust
pub enum GraphAction {
    // Host-level (returned to caller)
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
    ConnectionEvent { source_id: String, target_title: String, create: bool },
    ClearFocus,

    // Stateful (consumed by apply_action)
    PanUp, PanDown, PanLeft, PanRight,
    ZoomIn, ZoomOut, AutoFit,
    OpenSelected,
    MenuUp, MenuDown, MenuSelect, MenuClose,
}
```

- - -

## Configuration

### `Settings`

Top-level configuration struct, deserializable from TOML:

```rust
pub struct Settings {
    pub visual: VisualConfig,       // theme, colors, node/edge appearance
    pub physics: PhysicsConfig,     // simulation parameters
    pub interaction: InteractionConfig, // zoom, drag, double-click
    pub filter: FilterConfig,       // exclude tags/patterns, orphan toggle
    pub search: SearchConfig,       // result limits, popup dimensions
    pub display: DisplayConfig,     // status bar, border style
    pub legend: LegendConfig,       // position, max items
    pub editor: EditorConfig,       // editor command
    pub preview_enabled: bool,
    pub max_node: usize,            // default 500
}
```

Use `Settings::default()` for sensible defaults, then override fields. All sub-config structs also implement `Default`.

### `ThemeColors`

Resolved color palette based on settings (theme selection + hex overrides):

```rust
let theme = ThemeColors::resolve(&settings);
```

Built-in themes: default, Tokyo Night, Catppuccin Mocha, One Dark, Gruvbox, Dracula, Nord, Rose Pine, Everforest, Kanagawa, Solarized.

### Enums

All enums implement `Default`, `FromStr`, `Serialize`, and `Deserialize`:

| Enum | Variants |
|------|----------|
| `Background` | `Transparent` (default), `Solid` |
| `NodeColorMode` | `Tag` (default), `Folder`, `LinkCount`, `Uniform` |
| `EdgeColorMode` | `Source` (default), `Target`, `Uniform` |
| `LabelMode` | `Selected` (default), `Neighbors`, `All`, `None` |
| `NodeSizeMode` | `Fixed` (default), `LinkCount` |
| `CanvasMarker` | `Braille` (default), `HalfBlock`, `Dot` |
| `NodeShape` | `Circle` (default), `Square`, `Diamond` |
| `LegendPosition` | `TopRight` (default), `TopLeft`, `BottomRight`, `BottomLeft` |
| `PhysicsTickRate` | `Auto` (default), `Fixed` |

- - -

## Real-world example: clin-rs integration

[clin-rs](https://github.com/reekta92/clin-rs) embeds graf as a graph view overlay. The integration pattern:

1. **Map host config to `Settings`** — clin has its own config schema; an adapter function (`clin_settings`) builds `graf::Settings` field-by-field with exhaustive enum matches so new upstream variants break at compile time.

2. **Convert host data to `NodeSpec`** — clin's `NoteSummary` maps to `NodeSpec` (id, title, tags, folder, links).

3. **Build + start physics** — `GraphState::from_specs` + `start_physics`, stored as `Arc<RwLock<GraphState>>`.

4. **Render** — `draw_graph_view` in the ratatui draw loop, with host-owned status bar and search popup drawn separately.

5. **Input** — clin resolves its own keybinds first, then calls `apply_action` / `handle_graph_mouse` to drive the graph. Host-level actions (open file, toggle overlays) are handled by clin's event loop.

6. **Live editing** — `add_wikilink` / `remove_wikilink` for content transforms, `apply_connection_change` to update the live simulation without rebuilding.

7. **Focus modes** — `GraphState::from_specs` with a filtered subset of nodes, plus `ModeBanner` to indicate the active mode.

See [`src/graf_adapter.rs`](https://github.com/reekta92/clin-rs/blob/main/src/graf_adapter.rs) in clin-rs for the full implementation.

- - -

## Viewport

The `graf::viewport::Viewport` module is public and provides pan/zoom/hit-testing utilities:

```rust
use graf::viewport::Viewport;

let viewport = Viewport::default();

// Screen ↔ world coordinate conversion
let (wx, wy) = viewport.screen_to_world(col, row, canvas_area);
let (sx, sy) = viewport.world_to_screen(wx, wy, canvas_area);

// Auto-fit all nodes into view
let fitted = viewport.auto_fit_from_graph(simulation.get_graph(), 1.4 /* padding */);

// Hit-test: which node is at this world position?
let hit = viewport.hit_test(wx, wy, &graph_state, &settings, canvas_area, max_link_count);
```

- - -

## Thread safety

- `GraphState` is shared via `Arc<RwLock<GraphState>>` (`parking_lot::RwLock`).
- The physics thread takes write locks for simulation updates.
- The render path takes read locks.
- Input handlers (`handle_graph_keys`, `handle_graph_mouse`, `apply_action`) take their own write locks internally.
- `RenderCache` inside `GraphState` uses a `parking_lot::Mutex` for independent locking.
