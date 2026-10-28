# graf

A terminal-based force-directed graph visualizer for markdown wikilinks. Run `graf` in any directory and see your markdown files as an interactive, pannable, zoomable network graph.

# DISCLAIMER
`graf` is originally meant to be a feature for my main project `clin-rs`. When this project is fully developed it will be merged with the `clin-rs` project as a "graph view" feature. Currently **all the big features are implemented** and only the testing, bugfixing phase remains. So please create a issue for any problem you encounter using `graf`!

**This project is a library for the [clin-rs](https://github.com/reekta92/clin-rs) project though it can also be used as a standalone**

## Library API

For using `graf` as a library in your own ratatui application, see **[API.md](API.md)** — covers dependency setup, graph construction, physics, rendering, input handling, and configuration with real-world integration examples.

# Showcase
<table>
  <tr>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/bd3b94b3-db1c-49d8-818c-3b397f76730e" /></td>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/51a21b0e-261d-4d03-bd72-f6866a83a03d" /></td>
  </tr>
  <tr>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/34c3ade5-6d90-4d3b-9df1-d7b2bb400b0c" /></td>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/cae64831-faa3-445f-97e8-250be23bd05c" /></td>
  </tr>
  <tr>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/2a9b881d-518b-43e9-abec-763a3854ed95" /></td>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/cd2fd347-0e75-4d91-9833-063b1768e502" /></td>
  </tr>
  <tr>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/65a2c54c-c416-4b19-be6f-f305df9decda" /></td>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/a9fc970b-8cc3-47f7-8fd9-7a9534b1d367" /></td>
  </tr>
  <tr>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/b78004d4-e7ff-4bdf-a3a1-ad663f3c21ab" /></td>
    <td><img width="1888" height="1012" alt="image" src="https://github.com/user-attachments/assets/55e9f34a-9bfa-44e8-b147-8b9ff2dda365" /></td>
  </tr>
</table>

https://github.com/user-attachments/assets/de06ffda-a1f6-4317-9cd2-f7a222c13f18

- - -

## Features

**Link Parsing**

- **Links** — wikilinks (`[[target|label]]`), Markdown links (`[label](target.md)`), reference links, and note embeds. Links inside code and HTML comments are ignored.
- **Optional frontmatter** — reads YAML titles and tag lists, including block lists. Notes without frontmatter work normally.
- **Titles and filenames** — display title: frontmatter `title:` → first H1 → filename stem. Wikilinks prefer titles by default; filename-first resolution supports zk and Obsidian notebooks with ID filenames and duplicate titles.
- **Inline tags** — independently opt in to `#tags` and `:colon:tags:` for coloring, search, and filtering.

**Search**

- Press `f` to open a popup that fuzzy-matches against node titles, relative file paths, and tags (case-insensitive). Navigate results with arrows or Tab, press Enter to jump.

**Configuration**

- **Override order** — defaults → TOML config file (`~/.config/graf/config.toml`) → CLI arguments → `GRAF_*` environment variables.
- **11 built-in themes** — default, Tokyo Night, Catppuccin Mocha, One Dark, Gruvbox, Dracula, Nord, Rose Pine, Everforest, Kanagawa, Solarized.
- **Hex color overrides** — per-element (`node_color`, `edge_color`, `label_color`, etc.) on top of any theme.
- **Background modes** — `transparent` (passes through terminal transparency) or `solid` (theme background color).

**Overlays**

- **Minimap** - toggleable minimap that shows the entire canvas.
- **Status bar** — template-based with variables: `{files}`, `{links}`, `{selected}`, `{date}`, `{time}`, `{size}`, `{ratio}`.
- **Legend** — shows tag or folder color mapping; configurable position and max items; hidden when it would list a single entry.
- **Node scale** — `automatic` gives filled, roomy nodes on small vaults and the classic compact look on big ones; `small` and `large` pin either.
- **Selection focus** — selecting a node grows it and its neighbors (`grow`), grays out everything else (`dim`), both, or nothing.
- **Readable labels** — centered on the node, tinted with its color, and moved or dropped instead of overprinting a neighbor.
- **Grid overlay** — configurable number of divisions per axis.
- **Help overlay** — press `?` for a quick reference of all controls.

- - -

## Installation

### Prebuilt Linux archives
[GitHub Releases](https://github.com/reekta92/graf/releases) include installation instructions, Linux x86_64 and ARM64 CLI archives (`graf-rs-<version>-<target>.tar.xz` / `.tar.gz`), the Cargo source archive, and `SHA256SUMS`. Binary archives contain the executable at the root, `LICENSE`, and `README.md`; Linux builds require glibc 2.35 or newer. AUR maintainers can use these versioned archives and hashes in their PKGBUILDs. Each release publishes `graf-rs` to crates.io before publishing its GitHub release.

### Cargo

```bash
cargo install graf-rs --locked
```

### From source

```bash
git clone <repo-url>
cd graf
cargo install --path .
```

#### Dependencies

- Rust 1.88+ (Edition 2024)

- - -

## Usage

Navigate to any directory containing markdown files and run:

```bash
cd ~/docs/my-wiki
graf
```

`graf` recursively scans the current working directory for `*.md` and `*.mdx` files, resolves note links, then renders a force-directed graph in your terminal. Press `r` to rescan after external edits.

Double-click the same node or press `Enter` to open the linked file. Paths resolve against the scanned directory, including when using `--dir`. The editor is read from the `EDITOR` environment variable (defaults to `vim`).

```bash
EDITOR=nvim graf
```

- - -

## Controls

### Keyboard

| Key | Action |
|-----|--------|
| `↑` `↓` `←` `→` / `k` `j` `h` `l` | Move between nodes |
| `+` `=` / `Ctrl+J` | Zoom in |
| `-` / `Ctrl+K` | Zoom out |
| `Enter` | Open selected file in editor |
| `a` | Auto-fit view to all nodes |
| `f` | Activate search |
| `r` | Refresh file scan |
| `m` | Toggle minimap |
| `Ctrl+L` | Toggle legend |
| `Ctrl+R` | Reload config |
| `Ctrl+G` | Toggle grid |
| `Ctrl+S` | Toggle status bar |
| `?` | Toggle help overlay |
| `q` / `Esc` | Quit |

### Mouse

| Action | Effect |
|--------|--------|
| Scroll wheel | Zoom in/out |
| Click & drag background | Pan view (natural: drag up → view goes down) |
| Click & drag node | Reposition node |
| Single click | Select node |
| Double-click | Open file in editor |
| Double-click empty area | Deselect node |

### Search

Press `f` to activate the search popup. Search matches against node titles, file paths (relative), and tags (case-insensitive).

### Search controls

| Key | Action |
|-----|--------|
| `Enter` | Select result and jump to node |
| `↑` `↓` | Navigate results |
| `Tab` / `Shift+Tab` | Cycle forward/backward |
| `Esc` | Close search |
| `Ctrl+A` / `Ctrl+E` | Move to start/end of query |
| `Ctrl+U` | Clear entire query |
| `Ctrl+W` | Delete word backward |

- - -

## Configuration

Config file location: `~/.config/graf/config.toml`

If the file does not exist, `graf` creates the bundled default config. Every setting is optional — only include the ones you want to override. Reload errors retain the last valid settings; dismiss the error popup with a key, fix the file, then press Ctrl+R.

### Full example

```toml
max_node = 500

[visual]
theme = "catppuccin-mocha"
background = "transparent"
node_color_mode = "tag"
edge_color_mode = "source"
label_mode = "selected"
label_max_length = 20
node_size = 2.0
node_size_mode = "link_count"
node_scale = 5
node_fill = "dynamic"
selection_focus = "grow"
edge_thickness = 1
show_legend = true
show_grid = false
show_minimap = true
minimap_position = "bottom_right"
minimap_width = 30
minimap_height = 12
canvas_marker = "braille"
node_shape = "circle"
label_offset = 4.0
show_looking_glass = true
looking_glass_width = 24
looking_glass_height = 12

[visual.colors]
node_color = "#ff6600"
edge_color = "#333333"
label_color = "#aaaaaa"
selection_ring_color = "#ffffff"
border_color = "#555555"
grid_color = "#222222"

[physics]
ideal_distance = 80.0
damping = 0.95
max_iterations = 800
gravity = 0.01
cooling = true
prevent_overlapping = true
timestep = 0.016
thread_sleep_ms = 16
tick_rate = "auto"
[interaction]
double_click_ms = 300
zoom_factor = 1.15
drag_sensitivity = 0.5
auto_fit_padding = 1.4

[display]
show_status_bar = true
status_format = "Files: {files} | Links: {links} | Selected: {selected}"
border_style = "rounded"

[filter]
exclude_tags = ["draft", "private"]
exclude_patterns = ["*.bak", "archive/*"]
min_links = 0
show_orphan = false
[legend]
position = "top_right"
max_items = 10

[search]
max_results = 20
max_visible = 10
popup_width = 50
popup_y = 3

[editor]
command = ""
```

### `[visual]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `theme` | string | `"default"` | Color scheme preset (see **Themes** below) |
| `background` | string | `"transparent"` | `"transparent"` passes through terminal transparency, `"solid"` fills with theme background color |
| `node_color_mode` | string | `"tag"` | How nodes are colored: `"tag"`, `"folder"`, `"link_count"`, `"uniform"` |
| `edge_color_mode` | string | `"source"` | Edge color: `"source"`, `"target"`, `"uniform"` |
| `label_mode` | string | `"selected"` | Which labels to show: `"selected"`, `"neighbors"`, `"all"`, `"none"` |
| `label_max_length` | int | `20` | Max characters for title labels (1–60) |
| `node_size` | float | `2.0` | Base node radius (1–5) |
| `node_size_mode` | string | `"fixed"` | `"fixed"` constant size, `"link_count"` scales with connections |
| `edge_thickness` | int | `1` | Line thickness for edges (1–3) |
| `show_legend` | bool | `true` | Show tag legend |
| `show_grid` | bool | `false` | Show background grid overlay |
| `show_minimap` | bool | `true` | Show minimap in corner |
| `minimap_position` | string | `"top_right"` | Minimap position: `"top_right"`, `"top_left"`, `"bottom_right"`, `"bottom_left"` |
| `minimap_width` | int | `30` | Minimap width in columns |
| `minimap_height` | int | `12` | Minimap height in rows |
| `canvas_marker` | string | `"braille"` | Canvas rendering marker: `"braille"`, `"halfblock"`, `"dot"` |
| `minimap_marker` | string | `"braille"` | Minimap rendering marker: `"braille"`, `"halfblock"`, `"dot"` |
| `node_shape` | string | `"circle"` | Node shape: `"circle"`, `"square"`, `"diamond"` |
| `label_offset` | float | `4.0` | Distance between node edge and label |
| `show_looking_glass` | bool | `true` | Show looking glass |
| `looking_glass_width` | int | `24` | Looking glass width in columns |
| `looking_glass_height` | int | `12` | Looking glass height in rows |

### `[parsing]`

Standalone settings; the library's existing title-based graph and input APIs keep their contracts.

| Key | Default | Description |
|-----|---------|-------------|
| `link_resolution` | `"title_first"` | Wikilink lookup: `"title_first"` or `"filename_first"` |
| `link_format` | `"wiki"` | Format for newly created connections: `"wiki"` or `"markdown"`; both are always read |
| `hashtags` | `false` | Read inline `#tags`, including nested tags such as `#work/rust` |
| `colon_tags` | `false` | Read inline `:one:two:` tag chains |

For zk or an Obsidian vault using ID filenames:

```toml
[parsing]
link_resolution = "filename_first"
link_format = "wiki" # Use "markdown" for zk's default link format.
hashtags = true
colon_tags = false
```

`ahj2.md` with `# A Note on Titles` displays that title and accepts `[[ahj2|A Note on Titles]]` or `[A Note on Titles](ahj2)`. No note conversion or frontmatter IDs are required. Environment overrides are `GRAF_PARSING_LINK_RESOLUTION`, `GRAF_PARSING_LINK_FORMAT`, `GRAF_PARSING_HASHTAGS`, and `GRAF_PARSING_COLON_TAGS`. Reload with `Ctrl+R` to reparse existing files.

Explicit wikilink paths resolve from the notebook root; `./` and `../` paths resolve from the source note. Shortened paths and bare filenames must identify one note. Bare wikilinks try titles then filenames in `title_first`, and filenames then titles in `filename_first`. Titles and bare filenames match without case sensitivity; explicit paths preserve case. Ambiguous names produce a visible warning. Display filters, focused views, and node limits do not change resolution.

Markdown destinations resolve relative to their source file and support percent encoding, reference definitions, and omitted `.md`/`.mdx` extensions. Heading and block fragments connect to the containing note; same-note fragments, external URLs, and non-note attachments do not create edges.

Inline tags merge with frontmatter `tags` and `keywords`, retaining the first spelling and order. Code, HTML comments, escapes, URLs, and link labels are excluded. Tags may contain letters, numbers, `_`, `-`, and `/`; numeric-only tags are ignored. Inline tags can change which notes `exclude_tags` hides, so both options default to off.

New connections use an unambiguous title or full filename path for wikilinks, and an encoded relative path for Markdown. Deleting a connection removes references in both directions, preserving unrelated text, reference definitions, and code examples. Saves preserve permissions, reject stale content and paths outside the notebook, and replace each edited file atomically. A two-note deletion can partially succeed if the second file cannot be saved; graf reports the error and rescans the actual files.

This supports common file-based formats, not every tool-specific dialect: zk prefix/substring IDs and custom link templates, Bear multiword tags, Obsidian `%%` comments, and block-level graph nodes are not implemented. The graph still contains notes rather than headings or blocks.

### `[visual.colors]`

Optional hex color overrides applied on top of the selected theme. All fields are optional.

| Key | Type | Description |
|-----|------|-------------|
| `node_color` | hex string | Override all node colors (e.g. `"#ff6600"`) |
| `edge_color` | hex string | Override edge color |
| `label_color` | hex string | Override label text color |
| `selection_ring_color` | hex string | Override selection ring color |
| `border_color` | hex string | Override border, legend border, and minimap border |
| `title_color` | hex string | Override title color |
| `grid_color` | hex string | Override grid line color |
| `legend_text_color` | hex string | Override legend text color |
| `status_bar_color` | hex string | Override status bar text color |
| `background_color` | hex string | Override canvas and minimap background color |

### `[physics]`

Controls the force-directed layout simulation.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `ideal_distance` | float | `80.0` | Target distance between connected nodes |
| `damping` | float | `0.95` | Velocity damping per step (lower = faster settling) |
| `max_iterations` | int | `800` | Maximum steps per reheat when cooling is enabled |
| `gravity` | float | `0.01` | Center pull to prevent drift |
| `cooling` | bool | `true` | Settle and stop updating after cooling or the iteration limit; `false` keeps updating |
| `prevent_overlapping` | bool | `true` | Enable repulsive forces between nodes |
| `timestep` | float | `0.016` | Simulation timestep (finite, positive; lower = smoother but slower) |
| `thread_sleep_ms` | int | `16` | Minimum 1 ms between simulation ticks; auto mode uses at least 33 ms above 500 nodes |
| `tick_rate` | string | `"auto"` | Tick rate mode: `"auto"` or `"fixed"` |

### `[interaction]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `double_click_ms` | int | `300` | Time window in milliseconds to register a double-click |
| `zoom_factor` | float | `1.15` | Finite multiplier greater than 1 per scroll or key press |
| `drag_sensitivity` | float | `0.5` | Mouse drag speed multiplier |
| `auto_fit_padding` | float | `1.4` | Finite padding multiplier, at least 1; auto-fit uses actual canvas dimensions |

### `[display]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `show_status_bar` | bool | `true` | Show the status bar at the bottom |
| `status_format` | string | `"Files: {files} \| Links: {links} \| Selected: {selected}"` | Status bar text template. Variables: `{files}`, `{links}`, `{selected}`, `{date}`, `{time}`, `{size}`, `{ratio}` |
| `border_style` | string | `"rounded"` | Border style: `"plain"`, `"rounded"`, `"double"`, `"thick"`, `"dashed"`, `"dot"` |

### `[filter]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `exclude_tags` | array | `[]` | Tags to exclude from the graph |
| `exclude_patterns` | array | `[]` | Glob patterns for files to exclude |
| `min_links` | int | `0` | Minimum connections among displayed nodes; low-degree nodes are removed until every remaining node meets the threshold |
| `show_orphan` | bool | `false` | Whether to show orphan nodes |

### Limits and compatibility

`max_node` is a top-level integer (default `500`; `0` means unlimited).
Legacy config-file `[filter].max_nodes` is migrated when loading the standalone app.
The old `grid_divisions`, `drag_scale`, `border_title`, and `cursor_glyph` fields remain
readable for compatibility but are inactive: the grid adapts to zoom, panning uses canvas
size, the graph has no title border, and search highlights the cursor character.
Their old environment overrides are no longer supported.

### `[legend]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `position` | string | `"top_right"` | Legend position: `"top_right"`, `"top_left"`, `"bottom_right"`, `"bottom_left"` |
| `max_items` | int | `10` | Maximum tag groups to show in the legend |

### `[search]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `max_results` | int | `20` | Maximum search results to return |
| `max_visible` | int | `10` | Maximum results visible without scrolling |
| `popup_width` | int | `50` | Search popup width in columns |
| `popup_y` | int | `3` | Vertical position of search popup from top |

### `[editor]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `command` | string | `""` | Editor command. Falls back to `$EDITOR` env var, then `vim` |

- - -

## CLI Arguments

```
graf [OPTIONS]

Options:
  -d, --dir <DIR>
          Directory to scan for markdown files (default: current directory)
  -c, --config <CONFIG>
          Path to config file (default: XDG config dir)
      --theme <THEME>
          Theme preset to use
      --max-nodes <MAX_NODES>
          Maximum number of nodes to display
      --exclude <EXCLUDE>
          Exclude files matching glob pattern (repeatable)
      --exclude-tags <EXCLUDE_TAGS>
          Exclude tags (comma-separated)
      --node-color-mode <NODE_COLOR_MODE>
          Node color mode
      --edge-color-mode <EDGE_COLOR_MODE>
          Edge color mode
      --label-mode <LABEL_MODE>
          Label display mode
      --labels
          Show all labels
      --no-status
          Hide status bar
      --grid
          Show grid
      --no-minimap
          Hide minimap
      --no-legend
          Hide legend
      --background <BACKGROUND>
          Background style
      --border-style <BORDER_STYLE>
          Border style for overlays
      --editor <EDITOR>
          Editor command to open files
  -h, --help
          Print help
```

> Environment variables override CLI arguments, which override config file values, which override defaults. Invalid CLI values are rejected.

- - -

## Environment variables

Config values can be overridden with `GRAF_*` environment variables (uppercase, underscores):

```bash
GRAF_VISUAL_THEME="dracula" GRAF_FILTER_MAX_NODES=200 graf
```

Common variables:
- `GRAF_VISUAL_THEME` — Theme preset
- `GRAF_EDITOR_COMMAND` — Editor command
- `GRAF_FILTER_MAX_NODES` — Maximum nodes
- `GRAF_FILTER_MIN_LINKS` — Minimum links threshold
- `GRAF_VISUAL_NODE_COLOR_MODE` — Node color mode
- `GRAF_DISPLAY_SHOW_STATUS_BAR` — Show status bar (true/false)

Format: `GRAF_{SECTION}_{KEY}` in all caps with underscores.

- - -

## Themes

### Presets

| Value | Description |
|-------|-------------|
| `"default"` | Inherits colors from your terminal's color scheme. No hardcoded palette. |
| `"tokyo-night"` | Tokyo Night (dark) palette |
| `"catppuccin-mocha"` | Catppuccin Mocha palette |
| `"onedark"` | One Dark palette |
| `"gruvbox"` | Gruvbox Dark palette |
| `"dracula"` | Dracula palette |
| `"nord"` | Nord Frost palette |
| `"rose-pine"` | Rose Pine palette |
| `"everforest"` | Everforest palette |
| `"kanagawa"` | Kanagawa palette |
| `"solarized"` | Solarized Dark palette |

### Background modes

- `"transparent"` — The canvas background is not drawn, allowing your terminal's background (including transparency effects) to show through.
- `"solid"` — The canvas background is filled with the theme's configured background color. Only relevant when using a non-default theme.

- - -

## Comparison with Obsidian's Graph View

Obsidian's [graph view](https://help.obsidian.md/Plugins/Graph+view) was the direct inspiration for this project. Both share core capabilities: force-directed layout, node drag, pan/zoom, wikilink parsing, frontmatter tags, and search. The table below highlights where they differ:

| | Obsidian Graph View | graf |
|---|---|---|
| Platform | GUI (Electron desktop app) | Terminal (any emulator, SSH, tmux) |
| Input | Mouse | Mouse + full keyboard navigation |
| Node coloring | Tag (via community plugins) | Tag, folder, link count, uniform (built-in) |
| Node shapes | Circle only | Circle, square, diamond |
| Minimap | No | Yes (interactive, 4 positions) |
| Label modes | Hover to show | Selected, neighbors, all, none |
| Config | GUI settings | TOML file + CLI flags + env vars |
| Editor | Built-in Obsidian editor | Any `$EDITOR` (vim, nvim, helix, etc.) |
| Live file watching | Yes | Manual refresh (`r`) |


- - -

## How it works

### File scanning

`graf` starts at the current working directory and recursively collects all `*.md` and `*.mdx` files. Files matching patterns in `exclude_patterns` are skipped. All matching files are parsed before the graph applies top-level `max_node` (default 500, `0` means unlimited), keeping the most-connected notes and counting backlinks. Excluded directories also exclude their descendants.

### Link parsing

For each file, `graf` extracts:

1. **Links** — parsed Markdown and wikilink destinations, independent of display labels
2. **Title** — optional YAML `title`, the first H1, or the filename stem
3. **Tags** — YAML tag lists and enabled inline syntaxes

The document resolver maps destinations to relative file paths. The graph uses those paths as node identities, allowing duplicate display titles. Unresolved links are ignored; ambiguous links and unreadable or malformed notes produce warnings. A malformed note is skipped until corrected and rescanned.

Concurrent graf saves to the same note are rejected. A crashed save can leave a
`.note.md.graf-save` staging file; inspect it and ensure no save is active before removing it.
Changes from external editors are checked before replacement, but strict transactions require
all writers to share a lock.

### Color assignment

- **Tag mode**: Each unique tag across all files is assigned a unique color using golden-ratio distributed HSL values across the full 360° hue spectrum. No two tags ever share the same color.
- **Link count mode**: Colors are interpolated across the theme's node palette based on how many connections each node has.
- **Multi-tag nodes**: The primary (first) tag determines the main node color. Additional tags appear as small colored dots orbiting the node.

### Physics

A force-directed layout runs on a background thread at ~60fps using `fdg-sim`. Repulsive forces push all nodes apart, spring forces pull connected nodes together, and optional gravity prevents drift. With cooling enabled, updates stop after cooling or `max_iterations`; dragging reheats the graph. Disabling cooling keeps updates running.

- - -

## Tech stack

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI framework |
| `crossterm` | Terminal raw mode, mouse capture, alternate screen |
| `fdg-sim` | Force-directed graph simulation |
| `pulldown-cmark` | Markdown structure, wikilinks, and precise edit ranges |
| `serde-saphyr` | YAML frontmatter parsing |
| `percent-encoding` | Markdown path decoding and generation |
| `toml` + `serde` | Configuration loading |
| `clap` | CLI argument parsing |
| `glob` | File discovery |
| `chrono` | Date/time formatting for status bar |
| `directories` | XDG config path resolution |

- - -

```
graf/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs          # Entry point, terminal setup, event loop
    ├── lib.rs           # Library exports
    ├── cli.rs           # CLI argument definitions (clap)
    ├── app.rs           # App state management, graph initialization
    ├── config.rs        # Config loading, validation
    ├── settings.rs      # Setting structs and defaults
    ├── theme.rs         # Theme palettes and builder
    ├── linker.rs        # File discovery and legacy title-based library adapters
    ├── document.rs      # Markdown parsing, note resolution, and safe editing
    ├── wikilink.rs      # Shared content-level link editing
    ├── ui.rs            # Help overlay, search popup, config error rendering
    ├── util.rs          # Shared utilities
    ├── graph.rs         # GraphState, ForceGraph construction
    ├── render.rs        # Canvas drawing (nodes, edges, labels, legend, grid, minimap)
    ├── input.rs         # Keyboard and mouse event handling
    ├── viewport.rs      # Pan, zoom, screen-to-world conversion, hit-testing
    └── physics.rs       # Background thread for force simulation
```
- - -

## Performance

- Default simulation ticks every 16 ms; auto mode uses at least 33 ms above 500 nodes
- Cooling stops updates when alpha falls below `0.005` or the configured iteration limit is reached
- Files are sorted alphabetically by path before graph construction
- Zoom-in remains uncapped; filled shapes only sample the visible canvas. Zoom-out stops at 15% of the fitted zoom.
