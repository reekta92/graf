use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "graf",
    about = "Terminal force-directed graph visualizer for markdown notes"
)]
pub struct Cli {
    /// Directory to scan for markdown files (default: current directory)
    #[arg(short, long)]
    pub dir: Option<PathBuf>,

    /// Path to config file (default: XDG config dir)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Theme preset to use
    #[arg(long)]
    pub theme: Option<crate::settings::Theme>,

    /// Maximum number of nodes to display
    #[arg(long)]
    pub max_nodes: Option<usize>,

    /// Exclude files matching glob pattern (repeatable)
    #[arg(long, action = clap::ArgAction::Append)]
    pub exclude: Option<Vec<String>>,

    /// Exclude tags (comma-separated)
    #[arg(long)]
    pub exclude_tags: Option<String>,

    /// Node color mode
    #[arg(long)]
    pub node_color_mode: Option<crate::settings::NodeColorMode>,

    /// Edge color mode
    #[arg(long)]
    pub edge_color_mode: Option<crate::settings::EdgeColorMode>,

    /// Label display mode
    #[arg(long)]
    pub label_mode: Option<crate::settings::LabelMode>,

    /// Show all labels
    #[arg(long)]
    pub labels: bool,

    /// Hide status bar
    #[arg(long)]
    pub no_status: bool,

    /// Show grid
    #[arg(long)]
    pub grid: bool,

    /// Hide minimap
    #[arg(long)]
    pub no_minimap: bool,

    /// Hide legend
    #[arg(long)]
    pub no_legend: bool,

    /// Background style
    #[arg(long)]
    pub background: Option<crate::settings::Background>,

    /// Border style for overlays
    #[arg(long)]
    pub border_style: Option<crate::settings::BorderStyle>,

    /// Editor command to open files
    #[arg(long)]
    pub editor: Option<String>,

    /// Open the optional browser view directly (requires `graf-web`)
    #[arg(long)]
    pub web: bool,

    /// Show direction arrows on graph edges (off by default)
    #[arg(long)]
    pub with_arrows: bool,

    /// Arrowhead color as #RRGGBB (use with --with-arrows)
    #[arg(long, value_parser = parse_hex_color)]
    pub arrow_color: Option<String>,
}

fn parse_hex_color(value: &str) -> Result<String, String> {
    if crate::settings::parse_hex_color(value).is_some() {
        Ok(value.to_string())
    } else {
        Err("expected a hex color in #RRGGBB format".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_dir_alias_accepts_repeated_globs() {
        // upstream cli uses --exclude; test that it accepts multiple values
        let cli = Cli::try_parse_from([
            "graf",
            "--exclude",
            "docs/spec/**",
            "--exclude",
            "runtime/tests/**",
        ])
        .unwrap();
        assert_eq!(
            cli.exclude,
            Some(vec!["docs/spec/**".into(), "runtime/tests/**".into()])
        );
    }

    #[test]
    fn arrows_and_web_flags_parse() {
        let cli =
            Cli::try_parse_from(["graf", "--with-arrows", "--arrow-color", "#12aBcD", "--web"])
                .unwrap();
        assert!(cli.with_arrows && cli.web);
        assert_eq!(cli.arrow_color.as_deref(), Some("#12aBcD"));
        assert!(Cli::try_parse_from(["graf", "--arrow-color", "red"]).is_err());
    }
}
