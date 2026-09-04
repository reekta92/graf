use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ── Enums ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    #[default]
    Transparent,
    Solid,
}

impl FromStr for Background {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "transparent" => Ok(Background::Transparent),
            "solid" => Ok(Background::Solid),
            _ => Err(format!("Unknown background: {}", s)),
        }
    }
}

impl std::fmt::Display for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Background::Transparent => write!(f, "transparent"),
            Background::Solid => write!(f, "solid"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Default,
    TokyoNight,
    CatppuccinMocha,
    Onedark,
    Gruvbox,
    Dracula,
    Nord,
    RosePine,
    Everforest,
    Kanagawa,
    Solarized,
}

impl FromStr for Theme {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Theme::Default),
            "tokyo_night" | "tokyonight" => Ok(Theme::TokyoNight),
            "catppuccin_mocha" | "catppuccinmocha" => Ok(Theme::CatppuccinMocha),
            "onedark" => Ok(Theme::Onedark),
            "gruvbox" => Ok(Theme::Gruvbox),
            "dracula" => Ok(Theme::Dracula),
            "nord" => Ok(Theme::Nord),
            "rose_pine" | "rosepine" => Ok(Theme::RosePine),
            "everforest" => Ok(Theme::Everforest),
            "kanagawa" => Ok(Theme::Kanagawa),
            "solarized" => Ok(Theme::Solarized),
            _ => Err(format!("Unknown theme: {}", s)),
        }
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Theme::Default => write!(f, "default"),
            Theme::TokyoNight => write!(f, "tokyo_night"),
            Theme::CatppuccinMocha => write!(f, "catppuccin_mocha"),
            Theme::Onedark => write!(f, "onedark"),
            Theme::Gruvbox => write!(f, "gruvbox"),
            Theme::Dracula => write!(f, "dracula"),
            Theme::Nord => write!(f, "nord"),
            Theme::RosePine => write!(f, "rose_pine"),
            Theme::Everforest => write!(f, "everforest"),
            Theme::Kanagawa => write!(f, "kanagawa"),
            Theme::Solarized => write!(f, "solarized"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeColorMode {
    Tag,
    #[default]
    Folder,
    LinkCount,
    Uniform,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeColorMode {
    Source,
    Target,
    #[default]
    Uniform,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LabelMode {
    #[default]
    Selected,
    Neighbors,
    All,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeSizeMode {
    #[default]
    Fixed,
    LinkCount,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CanvasMarker {
    #[default]
    Braille,
    HalfBlock,
    Dot,
}

impl From<CanvasMarker> for ratatui::symbols::Marker {
    fn from(m: CanvasMarker) -> Self {
        match m {
            CanvasMarker::Braille => ratatui::symbols::Marker::Braille,
            CanvasMarker::HalfBlock => ratatui::symbols::Marker::HalfBlock,
            CanvasMarker::Dot => ratatui::symbols::Marker::Dot,
        }
    }
}
impl FromStr for NodeColorMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tag" => Ok(NodeColorMode::Tag),
            "folder" => Ok(NodeColorMode::Folder),
            "link_count" | "linkcount" => Ok(NodeColorMode::LinkCount),
            "uniform" => Ok(NodeColorMode::Uniform),
            _ => Err(format!("Unknown node_color_mode: {}", s)),
        }
    }
}

impl FromStr for EdgeColorMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "source" => Ok(EdgeColorMode::Source),
            "target" => Ok(EdgeColorMode::Target),
            "uniform" => Ok(EdgeColorMode::Uniform),
            _ => Err(format!("Unknown edge_color_mode: {}", s)),
        }
    }
}

impl FromStr for LabelMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "selected" => Ok(LabelMode::Selected),
            "neighbors" => Ok(LabelMode::Neighbors),
            "all" => Ok(LabelMode::All),
            "none" => Ok(LabelMode::None),
            _ => Err(format!("Unknown label_mode: {}", s)),
        }
    }
}

impl FromStr for NodeSizeMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fixed" => Ok(NodeSizeMode::Fixed),
            "link_count" | "linkcount" => Ok(NodeSizeMode::LinkCount),
            _ => Err(format!("Unknown node_size_mode: {}", s)),
        }
    }
}

impl FromStr for CanvasMarker {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "braille" => Ok(CanvasMarker::Braille),
            "half_block" | "halfblock" => Ok(CanvasMarker::HalfBlock),
            "dot" => Ok(CanvasMarker::Dot),
            _ => Err(format!("Unknown canvas_marker: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeShape {
    #[default]
    Circle,
    Square,
    Diamond,
}
impl FromStr for NodeShape {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "circle" => Ok(NodeShape::Circle),
            "square" => Ok(NodeShape::Square),
            "diamond" => Ok(NodeShape::Diamond),
            _ => Err(format!("Unknown node_shape: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LegendPosition {
    #[default]
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}
impl FromStr for LegendPosition {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "top_right" | "topright" => Ok(LegendPosition::TopRight),
            "top_left" | "topleft" => Ok(LegendPosition::TopLeft),
            "bottom_right" | "bottomright" => Ok(LegendPosition::BottomRight),
            "bottom_left" | "bottomleft" => Ok(LegendPosition::BottomLeft),
            _ => Err(format!("Unknown legend_position: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PhysicsTickRate {
    #[default]
    Auto,
    Fixed,
}

// ── Config Structs ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VisualConfig {
    pub background: Background,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub node_color_mode: NodeColorMode,
    #[serde(default)]
    pub edge_color_mode: EdgeColorMode,
    #[serde(default)]
    pub label_mode: LabelMode,
    pub label_max_length: usize,
    pub node_size: f64,
    #[serde(default)]
    pub node_size_mode: NodeSizeMode,
    pub edge_thickness: u16,
    pub show_legend: bool,
    #[serde(default)]
    pub show_minimap: bool,
    #[serde(default)]
    pub minimap_position: LegendPosition,
    pub minimap_width: u16,
    pub minimap_height: u16,
    #[serde(default)]
    pub canvas_marker: CanvasMarker,
    pub minimap_marker: CanvasMarker,
    #[serde(default)]
    pub node_shape: NodeShape,
    pub label_offset: f64,
    pub show_looking_glass: bool,
    pub looking_glass_width: u16,
    pub show_grid: bool,
    pub looking_glass_height: u16,
    #[serde(default)]
    pub colors: ColorOverrides,
    // Graf-only fields
    pub grid_divisions: usize,
}

impl Default for VisualConfig {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            background: Background::Solid,
            node_color_mode: NodeColorMode::Folder,
            edge_color_mode: EdgeColorMode::Uniform,
            label_mode: LabelMode::default(),
            label_max_length: 20,
            node_size: 2.0,
            node_size_mode: NodeSizeMode::default(),
            edge_thickness: 1,
            show_legend: true,
            show_minimap: false,
            minimap_position: LegendPosition::TopRight,
            minimap_width: 24,
            minimap_height: 12,
            canvas_marker: CanvasMarker::Braille,
            minimap_marker: CanvasMarker::Braille,
            node_shape: NodeShape::default(),
            show_grid: false,
            label_offset: 4.0,
            show_looking_glass: true,
            looking_glass_width: 24,
            looking_glass_height: 12,
            colors: ColorOverrides::default(),
            grid_divisions: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PhysicsConfig {
    pub ideal_distance: f64,
    #[serde(default)]
    pub tick_rate: PhysicsTickRate,
    // Graf-only fields
    pub timestep: f64,
    pub damping: f32,
    pub max_iterations: usize,
    pub gravity: f64,
    #[serde(default = "default_true")]
    pub cooling: bool,
    #[serde(default = "default_true")]
    pub prevent_overlapping: bool,
    pub thread_sleep_ms: u64,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            ideal_distance: 80.0,
            tick_rate: PhysicsTickRate::default(),
            timestep: 0.016,
            cooling: true,
            prevent_overlapping: true,
            damping: 0.95,
            max_iterations: 800,
            gravity: 0.01,
            thread_sleep_ms: 16,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct InteractionConfig {
    pub zoom_factor: f64,
    pub drag_sensitivity: f64,
    // Graf-only fields
    pub double_click_ms: u64,
    pub drag_scale: f64,
    pub auto_fit_padding: f64,
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self {
            zoom_factor: 1.15,
            drag_sensitivity: 1.0,
            double_click_ms: 300,
            drag_scale: 200.0,
            auto_fit_padding: 1.4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct FilterConfig {
    #[serde(default)]
    pub exclude_tags: Vec<String>,
    pub min_links: usize,
    pub max_nodes: usize,
    #[serde(default)]
    pub show_orphan: bool,
    // Graf-only fields
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchConfig {
    pub max_results: usize,
    pub max_visible: usize,
    // Graf-only fields
    pub popup_width: u16,
    pub popup_y: u16,
    pub cursor_glyph: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_results: 20,
            max_visible: 10,
            popup_width: 50,
            popup_y: 3,
            cursor_glyph: "▎".to_string(),
        }
    }
}
// Graf-specific config structs (needed for GrafConfig compatibility)
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BorderStyle {
    #[default]
    Plain,
    Rounded,
    Double,
    Thick,
    Dashed,
    #[serde(alias = "dotted")]
    Dot,
}
impl FromStr for BorderStyle {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "plain" => Ok(BorderStyle::Plain),
            "rounded" => Ok(BorderStyle::Rounded),
            "double" => Ok(BorderStyle::Double),
            "thick" => Ok(BorderStyle::Thick),
            "dashed" => Ok(BorderStyle::Dashed),
            "dotted" | "dot" => Ok(BorderStyle::Dot),
            _ => Err(format!("Unknown border_style: {}", s)),
        }
    }
}
impl BorderStyle {
    pub fn to_border_type(&self) -> ratatui::widgets::BorderType {
        match self {
            BorderStyle::Plain => ratatui::widgets::BorderType::Plain,
            BorderStyle::Rounded => ratatui::widgets::BorderType::Rounded,
            BorderStyle::Double => ratatui::widgets::BorderType::Double,
            BorderStyle::Thick => ratatui::widgets::BorderType::Thick,
            BorderStyle::Dashed => ratatui::widgets::BorderType::LightDoubleDashed,
            BorderStyle::Dot => ratatui::widgets::BorderType::LightTripleDashed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct DisplayConfig {
    #[serde(default = "default_true")]
    pub show_status_bar: bool,
    #[serde(default)]
    pub status_format: Option<String>,
    #[serde(default)]
    pub border_style: BorderStyle,
    #[serde(default = "default_border_title")]
    pub border_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct LegendConfig {
    #[serde(default)]
    pub position: LegendPosition,
    #[serde(default = "default_max_legend_items")]
    pub max_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct EditorConfig {
    #[serde(default)]
    pub command: Option<String>,
}

// Default functions for Graf-specific configs
fn default_true() -> bool { true }
fn default_border_title() -> String { "graf".to_string() }
fn default_max_legend_items() -> usize { 10 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    #[serde(default)]
    pub visual: VisualConfig,
    #[serde(default)]
    pub physics: PhysicsConfig,
    #[serde(default)]
    pub interaction: InteractionConfig,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub legend: LegendConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub preview_enabled: bool,
    pub max_node: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            visual: VisualConfig::default(),
            physics: PhysicsConfig::default(),
            interaction: InteractionConfig::default(),
            filter: FilterConfig::default(),
            search: SearchConfig::default(),
            display: DisplayConfig::default(),
            legend: LegendConfig::default(),
            editor: EditorConfig::default(),
            preview_enabled: false,
            max_node: 500,
        }
    }
}

// ── Color Overrides (graf version, richer than clin's) ───────────────────────

use ratatui::style::Color;
use serde::ser::Serializer;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorOverrides {
    pub node_color: Option<Color>,
    pub edge_color: Option<Color>,
    pub label_color: Option<Color>,
    pub selection_ring_color: Option<Color>,
    pub border_color: Option<Color>,
    pub title_color: Option<Color>,
    pub grid_color: Option<Color>,
    pub legend_text_color: Option<Color>,
    pub status_bar_color: Option<Color>,
    pub background_color: Option<Color>,
}

impl serde::Serialize for ColorOverrides {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        
        let mut s = serializer.serialize_struct("ColorOverrides", 10)?;
        
        fn fmt_color(c: &Color) -> String {
            if let Color::Rgb(r, g, b) = c {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("{:?}", c)
            }
        }
        
        if let Some(ref v) = self.node_color {
            s.serialize_field("node_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.edge_color {
            s.serialize_field("edge_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.label_color {
            s.serialize_field("label_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.selection_ring_color {
            s.serialize_field("selection_ring_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.border_color {
            s.serialize_field("border_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.title_color {
            s.serialize_field("title_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.grid_color {
            s.serialize_field("grid_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.legend_text_color {
            s.serialize_field("legend_text_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.status_bar_color {
            s.serialize_field("status_bar_color", &fmt_color(v))?;
        }
        if let Some(ref v) = self.background_color {
            s.serialize_field("background_color", &fmt_color(v))?;
        }
        
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for ColorOverrides {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ColorOverridesRaw {
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            node_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            edge_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            label_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            selection_ring_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            border_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            title_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            grid_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            legend_text_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            status_bar_color: Option<Color>,
            #[serde(default, deserialize_with = "deserialize_optional_color")]
            background_color: Option<Color>,
        }
        
        let raw = ColorOverridesRaw::deserialize(deserializer)?;
        Ok(ColorOverrides {
            node_color: raw.node_color,
            edge_color: raw.edge_color,
            label_color: raw.label_color,
            selection_ring_color: raw.selection_ring_color,
            border_color: raw.border_color,
            title_color: raw.title_color,
            grid_color: raw.grid_color,
            legend_text_color: raw.legend_text_color,
            status_bar_color: raw.status_bar_color,
            background_color: raw.background_color,
        })
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

pub fn deserialize_optional_color<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) => parse_hex_color(&s)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid hex color: {}", s))),
    }
}
