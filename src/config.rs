use std::fs;
// Re-export library types for binary compatibility
pub use crate::settings::{LabelMode, LegendPosition};
use std::path::PathBuf;

use directories::ProjectDirs;

// Type alias to use library Settings as GrafConfig
pub type GrafConfig = crate::settings::Settings;

impl GrafConfig {
    pub fn config_path() -> anyhow::Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "graf", "graf")
            .ok_or_else(|| anyhow::anyhow!("no home dir"))?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    pub fn theme_colors(&self) -> crate::theme::ThemeColors {
        crate::theme::ThemeColors::resolve(self)
    }

    pub fn expand_status(
        &self,
        files: usize,
        links: usize,
        selected: Option<&str>,
        viewport_size_pct: Option<f64>,
        viewport_ratio: Option<f64>,
    ) -> String {
        let fmt = self
            .display
            .status_format
            .as_deref()
            .unwrap_or("Files: {files} | Links: {links} | Selected: {selected}");
        let fmt = fmt.replace("{files}", &files.to_string());
        let fmt = fmt.replace("{links}", &links.to_string());
        let fmt = fmt.replace("{selected}", selected.unwrap_or("none"));
        let fmt = fmt.replace(
            "{date}",
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
        );
        let fmt = fmt.replace(
            "{time}",
            &chrono::Local::now().format("%H:%M:%S").to_string(),
        );
        let fmt = fmt.replace(
            "{size}",
            &format!("{:.0}%", viewport_size_pct.unwrap_or(0.0).clamp(0.0, 100.0)),
        );

        fmt.replace("{ratio}", &format!("{:.1}x", viewport_ratio.unwrap_or(1.0)))
    }

    /// Validate config values, return vec of error msgs.
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        if self.visual.label_max_length < 1 || self.visual.label_max_length > 60 {
            errs.push(format!(
                "visual.label_max_length must be 1-60, got {}",
                self.visual.label_max_length
            ));
        }
        if self.visual.node_size < 1.0 || self.visual.node_size > 5.0 {
            errs.push(format!(
                "visual.node_size must be 1.0-5.0, got {}",
                self.visual.node_size
            ));
        }
        if self.visual.edge_thickness < 1 || self.visual.edge_thickness > 3 {
            errs.push(format!(
                "visual.edge_thickness must be 1-3, got {}",
                self.visual.edge_thickness
            ));
        }
        if self.interaction.zoom_factor <= 0.0 {
            errs.push(format!(
                "interaction.zoom_factor must be > 0, got {}",
                self.interaction.zoom_factor
            ));
        }
        // Warn if legend and minimap would overlap in the same corner
        if self.visual.show_legend && self.visual.show_minimap {
            let same_corner = matches!(
                (&self.legend.position, &self.visual.minimap_position),
                (LegendPosition::TopRight, LegendPosition::TopRight)
                    | (LegendPosition::TopLeft, LegendPosition::TopLeft)
                    | (LegendPosition::BottomRight, LegendPosition::BottomRight)
                    | (LegendPosition::BottomLeft, LegendPosition::BottomLeft)
            );
            if same_corner {
                errs.push(
                    "legend.position and visual.minimap_position are in the same corner — they will overlap".to_string()
                );
            }
        }
        errs
    }

    pub fn load_from_path(path: Option<PathBuf>) -> (Self, Vec<String>, bool) {
        let mut config = Self::default();
        let mut errors = Vec::new();
        let mut created = false;

        if let Some(path) = path {
            if path.exists() {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        let migrated = migrate_toml(&content);
                        match toml::from_str::<GrafConfig>(&migrated) {
                            Ok(loaded) => config = loaded,
                            Err(e) => errors.push(format!("Invalid config TOML: {}", e)),
                        }
                    }
                    Err(e) => errors.push(format!("Cannot read config file: {}", e)),
                }
            } else if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
                let _ = fs::write(&path, generate_default_toml());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
                }
                created = true;
                if let Ok(loaded) = toml::from_str::<GrafConfig>(generate_default_toml()) {
                    config = loaded;
                }
            }
        }

        config.apply_env_overrides();

        (config, errors, created)
    }

    pub fn reload_from_path(path: Option<&PathBuf>) -> (Self, Vec<String>) {
        let mut config = Self::default();
        let mut errors = Vec::new();

        if let Some(path) = path {
            if path.exists() {
                match fs::read_to_string(path) {
                    Ok(content) => {
                        let migrated = migrate_toml(&content);
                        match toml::from_str::<GrafConfig>(&migrated) {
                            Ok(loaded) => config = loaded,
                            Err(e) => errors.push(format!("Invalid config TOML: {}", e)),
                        }
                    }
                    Err(e) => errors.push(format!("Cannot read config file: {}", e)),
                }
            } else {
                errors.push(format!("Config file not found: {}", path.display()));
            }
        } else {
            errors.push("No config file path available".to_string());
        }

        config.apply_env_overrides();

        (config, errors)
    }

    fn apply_env_overrides(&mut self) {
        use std::env;
        macro_rules! apply_enum {
            ($var:expr, $field:expr) => {
                if let Ok(s) = env::var(format!("GRAF_{}", $var)) {
                    if let Ok(v) = s.parse() {
                        $field = v;
                    }
                }
            };
        }
        macro_rules! apply_val {
            ($var:expr, $field:expr, $ty:ty) => {
                if let Ok(s) = env::var(format!("GRAF_{}", $var)) {
                    if let Ok(v) = s.parse::<$ty>() {
                        $field = v;
                    }
                }
            };
        }
        apply_enum!("VISUAL_THEME", self.visual.theme);
        apply_enum!("VISUAL_BACKGROUND", self.visual.background);
        apply_enum!("VISUAL_NODE_COLOR_MODE", self.visual.node_color_mode);
        apply_enum!("VISUAL_EDGE_COLOR_MODE", self.visual.edge_color_mode);
        apply_enum!("VISUAL_LABEL_MODE", self.visual.label_mode);
        apply_val!(
            "VISUAL_LABEL_MAX_LENGTH",
            self.visual.label_max_length,
            usize
        );
        apply_val!("VISUAL_NODE_SIZE", self.visual.node_size, f64);
        apply_enum!("VISUAL_NODE_SIZE_MODE", self.visual.node_size_mode);
        apply_val!("VISUAL_EDGE_THICKNESS", self.visual.edge_thickness, u16);
        apply_val!("VISUAL_SHOW_LEGEND", self.visual.show_legend, bool);
        apply_val!("VISUAL_SHOW_GRID", self.visual.show_grid, bool);
        apply_val!("VISUAL_SHOW_MINIMAP", self.visual.show_minimap, bool);
        apply_enum!("VISUAL_MINIMAP_POSITION", self.visual.minimap_position);
        apply_val!("VISUAL_MINIMAP_WIDTH", self.visual.minimap_width, u16);
        apply_val!("VISUAL_MINIMAP_HEIGHT", self.visual.minimap_height, u16);
        apply_enum!("VISUAL_CANVAS_MARKER", self.visual.canvas_marker);
        apply_enum!("VISUAL_MINIMAP_MARKER", self.visual.minimap_marker);
        apply_enum!("VISUAL_NODE_SHAPE", self.visual.node_shape);
        apply_val!("VISUAL_LABEL_OFFSET", self.visual.label_offset, f64);
        apply_val!("VISUAL_GRID_DIVISIONS", self.visual.grid_divisions, usize);
        apply_val!("PHYSICS_IDEAL_DISTANCE", self.physics.ideal_distance, f64);
        apply_val!("PHYSICS_DAMPING", self.physics.damping, f32);
        apply_val!("PHYSICS_MAX_ITERATIONS", self.physics.max_iterations, usize);
        apply_val!("PHYSICS_GRAVITY", self.physics.gravity, f64);
        apply_val!("PHYSICS_COOLING", self.physics.cooling, bool);
        apply_val!(
            "PHYSICS_PREVENT_OVERLAPPING",
            self.physics.prevent_overlapping,
            bool
        );
        apply_val!("PHYSICS_TIMESTEP", self.physics.timestep, f64);
        apply_val!("PHYSICS_THREAD_SLEEP_MS", self.physics.thread_sleep_ms, u64);
        apply_val!(
            "INTERACTION_DOUBLE_CLICK_MS",
            self.interaction.double_click_ms,
            u64
        );
        apply_val!("INTERACTION_ZOOM_FACTOR", self.interaction.zoom_factor, f64);
        apply_val!(
            "INTERACTION_DRAG_SENSITIVITY",
            self.interaction.drag_sensitivity,
            f64
        );
        apply_val!(
            "INTERACTION_AUTO_FIT_PADDING",
            self.interaction.auto_fit_padding,
            f64
        );
        apply_val!("INTERACTION_DRAG_SCALE", self.interaction.drag_scale, f64);
        apply_val!(
            "DISPLAY_SHOW_STATUS_BAR",
            self.display.show_status_bar,
            bool
        );
        if let Ok(s) = env::var("GRAF_DISPLAY_STATUS_FORMAT") {
            self.display.status_format = Some(s);
        }
        if let Ok(s) = env::var("GRAF_DISPLAY_BORDER_TITLE") {
            self.display.border_title = s;
        }
        if let Ok(s) = env::var("GRAF_FILTER_EXCLUDE_TAGS") {
            self.filter.exclude_tags = s.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(s) = env::var("GRAF_FILTER_EXCLUDE_PATTERNS") {
            self.filter.exclude_patterns = s.split(',').map(|s| s.trim().to_string()).collect();
        }
        apply_val!("FILTER_MIN_LINKS", self.filter.min_links, usize);
        apply_val!("FILTER_MAX_NODES", self.max_node, usize);
        apply_enum!("LEGEND_POSITION", self.legend.position);
        apply_val!("LEGEND_MAX_ITEMS", self.legend.max_items, usize);
        apply_val!("SEARCH_MAX_RESULTS", self.search.max_results, usize);
        apply_val!("SEARCH_MAX_VISIBLE", self.search.max_visible, usize);
        apply_val!("SEARCH_POPUP_WIDTH", self.search.popup_width, u16);
        apply_val!("SEARCH_POPUP_Y", self.search.popup_y, u16);
        if let Ok(s) = env::var("GRAF_SEARCH_CURSOR_GLYPH") {
            self.search.cursor_glyph = s;
        }
        if let Ok(s) = env::var("GRAF_EDITOR_COMMAND") {
            self.editor.command = Some(s);
        }
        if let Some(v) = env::var("GRAF_PREVIEW_ENABLED")
            .ok()
            .and_then(|s| s.parse::<bool>().ok())
        {
            self.preview_enabled = v;
        }
        if let Some(v) = env::var("GRAF_MAX_NODE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.max_node = v;
        }
    }
}

/// Load-time migration: legacy `filter.max_nodes` → top-level `max_node`.
fn migrate_toml(content: &str) -> String {
    let Ok(mut value) = content.parse::<toml::Value>() else {
        return content.to_string();
    };
    let Some(filter) = value.get_mut("filter").and_then(|f| f.as_table_mut()) else {
        return content.to_string();
    };
    let Some(old) = filter.remove("max_nodes") else {
        return content.to_string();
    };
    if let Some(table) = value.as_table_mut() {
        table.entry("max_node".to_string()).or_insert(old);
    }
    value.to_string()
}

fn generate_default_toml() -> &'static str {
    include_str!("default_config.toml")
}
