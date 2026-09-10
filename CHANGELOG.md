# Changelog

All notable changes to graf are documented in this file.

## [1.1.0] - 2026-09-10

### Added

- Use concentric halos for extra tags
- Segmented border tag rendering
- Tag wedges inside nodes
- Grow ring border with slight enlarge
- Node_fill config (dynamic|filled|none)
- Smoothstep automatic scale curve
- Integer node_scale 1-10
- Make node scale and selection focus configurable
- Solid nodes, label collision handling, focus dimming, auto-fit on settle
- Added menu_shortcut_color to match pinstar shortcut style
- Styling updates to match pinstar context menu

### CI

- Applied recommended clippy fixes

### Fixed

- Tags now use local colors
- Reverted tag rendering logic
- Fixed filled nodes not rendering when zoomed out
- Scale halo/ring spacing proportionally to node radius for artifact-free rendering
- Draw node body after halos for prominence at low LOD
- Restore looking glass bounds dropped during refactor
- Fill square and diamond node shapes
- Dense cell-aligned node fill
- Strip verbose comments from default config
- Grow multiplier uses actual node size
- Dim entire graph outside selection neighborhood
- Fixed toggle shortcuts not working
- Use theme background for context menu

### Miscellaneous

- Added API.md and updated README.md
- Updated README
- README.md updates

### Styling

- Ran cargo fmt
- Cargo fmt
- Cargo fmt
- Cargo fmt
## [1.0.0] - 2026-09-05

### CI

- Remove unsupported inputs
- Added permission write

### Miscellaneous

- Bump directories from 5.0.1 to 6.0.0
- Bump petgraph from 0.6.5 to 0.8.3
- Bump toml from 0.8.23 to 1.1.4+spec-1.1.0

### Styling

- Cargo fmt

### Release

- V1.0.0
## [0.5.0] - 2026-09-04

### Added

- Re-export physics/render entry points for embedders
- Injectable GraphKeymap for handle_graph_keys

### CI

- Use central workflows from .github repo
- General ci fixes for migration
- Routed all ci/cd channels to the central repo

### Fixed

- Drop unreachable wildcard in CanvasMarker conversion
- Root label missing from the legend

### Miscellaneous

- Add beta labeler workflow caller
- Bump glob from 0.3.3 to 0.3.4
- Bump crossterm from 0.28.1 to 0.29.0
- Bump chrono from 0.4.44 to 0.4.45
- Update readme
- Update readme
- Update readme
- Updated documentation
- Update readme
- Version update
## [0.4.16] - 2026-05-03

### Fixed

- Optimization changes
- Changed dots with halfblocks in the new minimap
- Rewritten minimap logic for optimization and flickering fix

### Miscellaneous

- Update readme
- Update version, readme
- Update version for new release
- Update version for new release
- Update version for new release
- Update version for new release
- Update version for new release
- Update version for new release
- Update version for new release
- Updated default config
- PKGBUILD update
- Readme update
- Readme update

### Add

- Hjkl movement
- Hjkl movement
- Hjkl movement
## [0.4.14-2] - 2026-05-03

### Miscellaneous

- Update version for new release
- Updated default config
- Updated default config
- Updated default config
- Updated default config
- Readme update
- Readme update
- Readme update
- Readme update
- Readme update
## [0.4.12] - 2026-05-02

### Fixed

- Solarized theme grid color
- Minimap doesn't show the entire canvas
- Minimap doesn't show the entire canvas

### Miscellaneous

- Readme update
- Readme update
- Readme update

### Add

- Hot reloading the config
## [0.4.11] - 2026-05-02

### Fixed

- Background color now renders properly and added more themes
- Better default configs for user accessibility
- Fixed jittering when dragging the nodes

### Miscellaneous

- Readme update
- Readme update
- Readme update

### Add

- More configuration options
- More configuration options
- Minimap improvements and search functionality
- Minimap
- Configuration options, color themes, legend and more

### Finalization

- Last fixes, improvements and refactoring
- Last fixes, improvements and refactoring

