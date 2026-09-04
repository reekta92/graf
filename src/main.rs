mod app;
mod cli;
mod config;
// Engine modules are duplicated from the lib target; items unused by the bin
// are part of the lib API surface.
#[allow(dead_code)]
mod graph;
#[allow(dead_code)]
mod linker;
mod ui;
mod util;
#[allow(dead_code)]
mod settings;
#[allow(dead_code)]
mod theme;
#[allow(dead_code)]
mod physics;
#[allow(dead_code)]
mod viewport;
#[allow(dead_code)]
mod render;
#[allow(dead_code)]
mod input;
#[allow(dead_code)]
mod wikilink;

use std::io;
use std::io::Write;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::input::GraphAction;

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    fn as_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    fn suspend(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        io::stdout().flush()?;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;
        self.terminal.clear()?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
        let _ = io::stdout().flush();
    }
}

enum EventAction {
    Quit,
    OpenFile(String),
    ReloadConfig,
}

struct ReloadCtx<'a> {
    config_path: &'a Option<std::path::PathBuf>,
    cli: &'a cli::Cli,
    cwd: &'a std::path::PathBuf,
}

fn dispatch_action(
    action: EventAction,
    app_state: &mut app::AppState,
    guard: &mut TerminalGuard,
    config: &mut config::GrafConfig,
    running: &mut bool,
    reload_ctx: &ReloadCtx<'_>,
) -> Result<()> {
    match action {
        EventAction::Quit => {
            app_state.shutdown();
            *running = false;
        }
        EventAction::OpenFile(path) => {
            guard.suspend()?;
            open_file_in_editor(&path, config);
            guard.resume()?;
        }
        EventAction::ReloadConfig => {
            let old_physics = config.physics.clone();
            let old_filter = config.filter.clone();

            let (new_config, errors) =
                config::GrafConfig::reload_from_path(reload_ctx.config_path.as_ref());
            *config = new_config;
            apply_cli_overrides(config, reload_ctx.cli);
            let validation_errors = config.validate();

            if errors.is_empty() && validation_errors.is_empty() {
                app_state.config_reload_msg = Some("Config reloaded".to_string());
                app_state.config_errors.clear();
            } else {
                let mut all_errs = errors;
                all_errs.extend(validation_errors);
                app_state.config_errors = all_errs.clone();
                app_state.config_reload_msg =
                    Some(format!("Config error: {}", all_errs.join("; ")));
            }
            app_state.config_reload_ttl = 60;

            if config.physics != old_physics {
                app_state.refresh_simulation(config);
            }
            if config.filter != old_filter {
                let files = linker::scan_markdown_files(
                    reload_ctx.cwd,
                    &config.filter.exclude_patterns,
                    config.max_node,
                );
                app_state.files = files;
                app_state.refresh_simulation(config);
            }

            app_state.show_minimap = config.visual.show_minimap;
            app_state.show_legend = config.visual.show_legend;
            app_state.show_grid = config.visual.show_grid;
            app_state.show_status_bar = config.display.show_status_bar;
        }
    }
    Ok(())
}

fn handle_event(
    ev: Event,
    app_state: &mut app::AppState,
    config: &config::GrafConfig,
    guard: &TerminalGuard,
) -> Result<Option<EventAction>> {
    match ev {
        Event::Key(key) => {
            if app_state.show_help {
                if key.code == crossterm::event::KeyCode::Esc
                    || key.code == crossterm::event::KeyCode::Char('?')
                {
                    app_state.show_help = false;
                }
                return Ok(None);
            }

            if app_state.search_active {
                handle_search_keys(app_state, key, config);
                return Ok(None);
            }

            if let Some(graph_state) = &app_state.graph_state
                && let Some(action) = crate::input::handle_graph_keys(graph_state, key, config, &app_state.keymap)
            {
                return Ok(apply_graph_action(action, app_state, config));
            }
            Ok(None)
        }
        Event::Mouse(mouse_event) => {
            if app_state.show_help || app_state.search_active {
                return Ok(None);
            }
            if let Some(graph_state) = &app_state.graph_state
                && let Some(action) = crate::input::handle_graph_mouse(
                    graph_state,
                    mouse_event,
                    frame_area(guard)?,
                    &mut app_state.graph_mouse_state,
                    config,
                    app_state.show_status_bar,
                )
            {
                return Ok(apply_graph_action(action, app_state, config));
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn apply_graph_action(
    action: GraphAction,
    app_state: &mut app::AppState,
    config: &config::GrafConfig,
) -> Option<EventAction> {
    match action {
        GraphAction::Quit => Some(EventAction::Quit),
        GraphAction::ToggleHelp => {
            app_state.show_help = true;
            None
        }
        GraphAction::ToggleSearch => {
            app_state.search_active = true;
            None
        }
        GraphAction::ToggleMinimap => {
            app_state.show_minimap = !app_state.show_minimap;
            None
        }
        GraphAction::ToggleLegend => {
            app_state.show_legend = !app_state.show_legend;
            None
        }
        GraphAction::ToggleGrid => {
            app_state.show_grid = !app_state.show_grid;
            None
        }
        GraphAction::ToggleStatus => {
            app_state.show_status_bar = !app_state.show_status_bar;
            None
        }
        GraphAction::OpenFile(path) => Some(EventAction::OpenFile(path)),
        GraphAction::Refresh => {
            app_state.refresh_simulation(config);
            None
        }
        GraphAction::ReloadConfig => Some(EventAction::ReloadConfig),
        GraphAction::MenuAction(item) => {
            execute_menu_action(app_state, config, item);
            None
        }
        GraphAction::ConnectionEvent {
            source_id,
            target_title,
            create,
        } => {
            apply_connection(app_state, &source_id, &target_title, create);
            None
        }
        GraphAction::ClearFocus => {
            app_state.exit_focus(config);
            None
        }
        // Preview/looking-glass are clin-host features.
        GraphAction::TogglePreview | GraphAction::ToggleLookingGlass => None,
        // Stateful actions are consumed inside the engine.
        _ => None,
    }
}

fn execute_menu_action(
    app_state: &mut app::AppState,
    config: &config::GrafConfig,
    item: crate::graph::MenuItem,
) {
    use crate::graph::MenuItem;
    use crate::graph::ModeBanner;
    match item {
        MenuItem::CreateConnection => {
            if let Some(gs) = &app_state.graph_state {
                let mut g = gs.write();
                if let Some(src) = g.selection.primary {
                    g.connection_source = Some(src);
                    g.mode_banner = Some(ModeBanner::CreateConnection);
                    g.context_menu = None;
                }
            }
        }
        MenuItem::DeleteConnection => {
            if let Some(gs) = &app_state.graph_state {
                let mut g = gs.write();
                if let Some(src) = g.selection.primary {
                    g.deleting_connection_source = Some(src);
                    g.mode_banner = Some(ModeBanner::DeleteConnection);
                    g.context_menu = None;
                }
            }
        }
        MenuItem::LocalGraph => {
            let ids: std::collections::HashSet<String> = {
                let Some(gs) = &app_state.graph_state else {
                    return;
                };
                let g = gs.read();
                let mut ids = std::collections::HashSet::new();
                if let Some(anchor) = g.selection.primary {
                    let graph_ref = g.simulation.get_graph();
                    if let Some(n) = graph_ref.node_weight(anchor) {
                        ids.insert(n.data.id.clone());
                    }
                    for nbr in graph_ref.neighbors(anchor) {
                        if let Some(n) = graph_ref.node_weight(nbr) {
                            ids.insert(n.data.id.clone());
                        }
                    }
                }
                ids
            };
            if !ids.is_empty() {
                app_state.enter_focus(config, ids, ModeBanner::LocalGraph);
            }
        }
        MenuItem::ShowGroup => {
            let ids: std::collections::HashSet<String> = {
                let Some(gs) = &app_state.graph_state else {
                    return;
                };
                let g = gs.read();
                g.selection
                    .extra
                    .iter()
                    .chain(g.selection.primary.iter())
                    .filter_map(|idx| g.simulation.get_graph().node_weight(*idx))
                    .map(|n| n.data.id.clone())
                    .collect()
            };
            if !ids.is_empty() {
                app_state.enter_focus(config, ids, ModeBanner::GroupedGraph);
            }
        }
        MenuItem::DeleteNode => {
            let ids: Vec<String> = {
                let Some(gs) = &app_state.graph_state else {
                    return;
                };
                let g = gs.read();
                let mut v = Vec::new();
                if let Some(idx) = g.selection.primary
                    && let Some(n) = g.simulation.get_graph().node_weight(idx)
                {
                    v.push(n.data.id.clone());
                }
                for idx in &g.selection.extra {
                    if let Some(n) = g.simulation.get_graph().node_weight(*idx) {
                        let id = n.data.id.clone();
                        if !v.contains(&id) {
                            v.push(id);
                        }
                    }
                }
                v
            };
            // graf bin has no note storage: drop the notes from the graph
            // view only (files on disk are untouched).
            app_state.files.retain(|f| !ids.contains(&f.relative_path));
            app_state.refresh_simulation(config);
        }
    }
}

fn apply_connection(
    app_state: &mut app::AppState,
    source_id: &str,
    target_title: &str,
    create: bool,
) {
    // Direction resolution for delete: if the source note does not link to the
    // target but the target links to the source, edit the target instead.
    let mut resolved_source = source_id.to_string();
    let mut resolved_target = target_title.to_string();
    if !create {
        let source_has_link = app_state.files.iter().any(|f| {
            f.relative_path == source_id
                && f.wikilinks
                    .iter()
                    .any(|l| l.eq_ignore_ascii_case(target_title))
        });
        if !source_has_link {
            let target_file = app_state
                .files
                .iter()
                .find(|f| f.title.eq_ignore_ascii_case(target_title));
            if let Some(target_file) = target_file {
                let target_id = target_file.relative_path.clone();
                if let Some(source_file) = app_state
                    .files
                    .iter()
                    .find(|f| f.relative_path == source_id)
                {
                    let source_title = source_file.title.clone();
                    if target_file
                        .wikilinks
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(&source_title))
                    {
                        resolved_source = target_id;
                        resolved_target = source_title;
                    }
                }
            }
        }
    }

    // 1. Write the wikilink edit to the file on disk.
    let path = app_state.base_dir.join(&resolved_source);
    if let Ok(content) = std::fs::read_to_string(&path) {
        let new_content = if create {
            crate::wikilink::add_wikilink(&content, &resolved_target)
        } else {
            crate::wikilink::remove_wikilink(&content, &resolved_target)
        };
        if new_content != content {
            let _ = std::fs::write(&path, &new_content);
        }
    }

    // 2. Keep the file list in sync until the next full rescan.
    if let Some(f) = app_state
        .files
        .iter_mut()
        .find(|f| f.relative_path == resolved_source)
    {
        if create {
            if !f
                .wikilinks
                .iter()
                .any(|l| l.eq_ignore_ascii_case(&resolved_target))
            {
                f.wikilinks.push(resolved_target.clone());
            }
        } else {
            f.wikilinks
                .retain(|l| !l.eq_ignore_ascii_case(&resolved_target));
        }
    }

    // 3. Apply the edge to the live simulation (no rebuild: positions and
    // viewport are preserved).
    if let Some(gs) = app_state.graph_state.as_ref() {
        let mut g = gs.write();
        let graph = g.simulation.get_graph();
        let src = graph
            .node_indices()
            .find(|&i| graph[i].data.id == resolved_source);
        let tgt = graph
            .node_indices()
            .find(|&i| graph[i].data.title.eq_ignore_ascii_case(&resolved_target));
        if let (Some(s), Some(t)) = (src, tgt) {
            crate::graph::apply_connection_change(&mut g.simulation, s, t, create);
            let mut cache = g.render_cache.lock();
            cache.topology_dirty = true;
            cache.minimap_dirty = true;
        }
    }
}

fn apply_cli_overrides(config: &mut config::GrafConfig, cli: &cli::Cli) {
    if let Some(ref theme) = cli.theme
        && let Ok(t) = theme.parse()
    {
        config.visual.theme = t;
    }
    if let Some(max) = cli.max_nodes {
        config.max_node = max;
    }
    if let Some(ref patterns) = cli.exclude {
        config.filter.exclude_patterns = patterns.clone();
    }
    if let Some(ref tags) = cli.exclude_tags {
        config.filter.exclude_tags = tags.split(',').map(|s| s.trim().to_string()).collect();
    }
    if let Some(ref mode) = cli.node_color_mode
        && let Ok(m) = mode.parse()
    {
        config.visual.node_color_mode = m;
    }
    if let Some(ref mode) = cli.edge_color_mode
        && let Ok(m) = mode.parse()
    {
        config.visual.edge_color_mode = m;
    }
    if cli.labels {
        config.visual.label_mode = config::LabelMode::All;
    }
    if let Some(ref mode) = cli.label_mode
        && let Ok(m) = mode.parse()
    {
        config.visual.label_mode = m;
    }
    if cli.no_status {
        config.display.show_status_bar = false;
    }
    if cli.grid {
        config.visual.show_grid = true;
    }
    if cli.no_minimap {
        config.visual.show_minimap = false;
    }
    if cli.no_legend {
        config.visual.show_legend = false;
    }
    if let Some(ref bg) = cli.background
        && let Ok(b) = bg.parse()
    {
        config.visual.background = b;
    }
    if let Some(ref style) = cli.border_style
        && let Ok(s) = style.parse()
    {
        config.display.border_style = s;
    }
    if let Some(ref editor) = cli.editor {
        config.editor.command = Some(editor.clone());
    }
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    let config_path = cli
        .config
        .clone()
        .or_else(|| config::GrafConfig::config_path().ok());
    let (mut config, mut config_errors, config_created) =
        config::GrafConfig::load_from_path(config_path.clone());
    if config_created {
        eprintln!(
            "Created default config at {}",
            config::GrafConfig::config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "~/.config/graf/config.toml".into())
        );
    }
    apply_cli_overrides(&mut config, &cli);

    config_errors.extend(config.validate());

    let cwd = if let Some(ref dir) = cli.dir {
        dir.canonicalize()
            .context(format!("failed to resolve directory: {}", dir.display()))?
    } else {
        std::env::current_dir().context("failed to get current directory")?
    };
    let files = linker::scan_markdown_files(
        &cwd,
        &config.filter.exclude_patterns,
        config.max_node,
    );

    if files.is_empty() {
        eprintln!("No markdown files found in {}", cwd.display());
        std::process::exit(1);
    }

    let mut guard = TerminalGuard::new()?;
    let mut app_state = app::AppState::new(&config, cwd.clone(), files, config_errors);
    let mut running = true;

    let reload_ctx = ReloadCtx {
        config_path: &config_path,
        cli: &cli,
        cwd: &cwd,
    };

    while running {
        guard.as_mut().draw(|frame| {
            ui::draw_ui(frame, &app_state, &config);
        })?;

        if event::poll(std::time::Duration::from_millis(16))? {
            let ev = event::read()?;
            if let Some(action) = handle_event(ev, &mut app_state, &config, &guard)? {
                dispatch_action(
                    action,
                    &mut app_state,
                    &mut guard,
                    &mut config,
                    &mut running,
                    &reload_ctx,
                )?;
            }
            while event::poll(std::time::Duration::ZERO)? {
                let ev = event::read()?;
                if let Some(action) = handle_event(ev, &mut app_state, &config, &guard)? {
                    if matches!(action, EventAction::Quit) {
                        dispatch_action(
                            action,
                            &mut app_state,
                            &mut guard,
                            &mut config,
                            &mut running,
                            &reload_ctx,
                        )?;
                        break;
                    }
                    dispatch_action(
                        action,
                        &mut app_state,
                        &mut guard,
                        &mut config,
                        &mut running,
                        &reload_ctx,
                    )?;
                }
            }
        }

        if app_state.config_reload_ttl > 0 {
            app_state.config_reload_ttl -= 1;
            if app_state.config_reload_ttl == 0 {
                app_state.config_reload_msg = None;
            }
        }
    }

    Ok(())
}

fn frame_area(guard: &TerminalGuard) -> Result<ratatui::layout::Rect> {
    let size = guard
        .terminal
        .size()
        .context("failed to get terminal size")?;
    Ok(ratatui::layout::Rect::new(0, 0, size.width, size.height))
}

fn open_file_in_editor(relative_path: &str, config: &config::GrafConfig) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let full_path = cwd.join(relative_path);

    let full_path = match full_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return,
    };

    if !full_path.starts_with(&cwd) {
        return;
    }

    let editor = if let Some(cmd) = &config.editor.command {
        if !cmd.is_empty() {
            cmd.clone()
        } else {
            std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string())
        }
    } else {
        std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string())
    };

    if let Err(e) = std::process::Command::new(&editor).arg(&full_path).status() {
        eprintln!("Failed to open editor '{}': {}", editor, e);
    }
}

fn handle_search_keys(
    app_state: &mut app::AppState,
    key: crossterm::event::KeyEvent,
    config: &config::GrafConfig,
) {
    use crossterm::event::{KeyCode, KeyModifiers};

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        KeyCode::Esc => {
            app_state.search_active = false;
            app_state.search_query.clear();
            app_state.search_results.clear();
            app_state.search_selected = 0;
            app_state.search_cursor = 0;
        }
        KeyCode::Enter => {
            if let Some(&(idx, _)) = app_state.search_results.get(app_state.search_selected) {
                let (nx, ny) = if let Some(graph_state) = &app_state.graph_state {
                    let guard = graph_state.read();
                    let graph = guard.simulation.get_graph();
                    if let Some(node) = graph.node_weight(idx) {
                        (node.location.x as f64, node.location.y as f64)
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                };
                let Some(graph_state) = &app_state.graph_state else {
                    return;
                };
                let mut guard = graph_state.write();
                guard.selection.select_only(idx);
                guard.viewport.center_on_node(nx as f32, ny as f32);
            }
            app_state.search_active = false;
            app_state.search_query.clear();
            app_state.search_results.clear();
            app_state.search_selected = 0;
            app_state.search_cursor = 0;
        }
        KeyCode::Up => {
            if app_state.search_selected > 0 {
                app_state.search_selected -= 1;
            }
        }
        KeyCode::Down => {
            if !app_state.search_results.is_empty()
                && app_state.search_selected < app_state.search_results.len() - 1
            {
                app_state.search_selected += 1;
            }
        }
        KeyCode::Tab if shift => {
            if !app_state.search_results.is_empty() {
                app_state.search_selected = app_state
                    .search_selected
                    .checked_sub(1)
                    .unwrap_or(app_state.search_results.len() - 1);
            }
        }
        KeyCode::Tab => {
            if !app_state.search_results.is_empty() {
                app_state.search_selected =
                    (app_state.search_selected + 1) % app_state.search_results.len();
            }
        }
        KeyCode::Backspace => {
            if app_state.search_cursor > 0 {
                let prev = app_state.search_query[..app_state.search_cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                app_state
                    .search_query
                    .replace_range(prev..app_state.search_cursor, "");
                app_state.search_cursor = prev;
                run_search(app_state, config);
            }
        }
        KeyCode::Delete => {
            if app_state.search_cursor < app_state.search_query.len() {
                let next = app_state.search_query[app_state.search_cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| app_state.search_cursor + i)
                    .unwrap_or(app_state.search_query.len());
                app_state
                    .search_query
                    .replace_range(app_state.search_cursor..next, "");
                run_search(app_state, config);
            }
        }
        KeyCode::Left => {
            if app_state.search_cursor > 0 {
                app_state.search_cursor = app_state.search_query[..app_state.search_cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
        }
        KeyCode::Right => {
            if app_state.search_cursor < app_state.search_query.len() {
                app_state.search_cursor = app_state.search_query[app_state.search_cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| app_state.search_cursor + i)
                    .unwrap_or(app_state.search_query.len());
            }
        }
        KeyCode::Home => {
            app_state.search_cursor = 0;
        }
        KeyCode::End => {
            app_state.search_cursor = app_state.search_query.len();
        }
        KeyCode::Char('h') if ctrl => {
            delete_word_back(app_state);
            run_search(app_state, config);
        }
        KeyCode::Char('w') if ctrl => {
            delete_word_back(app_state);
            run_search(app_state, config);
        }
        KeyCode::Char('u') if ctrl => {
            app_state.search_query.clear();
            app_state.search_cursor = 0;
            run_search(app_state, config);
        }
        KeyCode::Char('a') if ctrl => {
            app_state.search_cursor = 0;
        }
        KeyCode::Char('e') if ctrl => {
            app_state.search_cursor = app_state.search_query.len();
        }
        KeyCode::Char(c) if !ctrl => {
            const MAX_SEARCH_LEN: usize = 256;
            if app_state.search_query.len() < MAX_SEARCH_LEN {
                app_state.search_query.insert(app_state.search_cursor, c);
                app_state.search_cursor += c.len_utf8();
                run_search(app_state, config);
            }
        }
        _ => {}
    }
}

fn delete_word_back(app_state: &mut app::AppState) {
    if app_state.search_cursor == 0 {
        return;
    }
    let query = &app_state.search_query[..app_state.search_cursor];
    let trimmed = query.trim_end_matches(|c: char| c.is_whitespace());
    let cut_to = trimmed
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    app_state
        .search_query
        .replace_range(cut_to..app_state.search_cursor, "");
    app_state.search_cursor = cut_to;
}

fn run_search(app_state: &mut app::AppState, config: &config::GrafConfig) {
    if let Some(graph_state) = &app_state.graph_state {
        let guard = graph_state.read();
        app_state.search_results = graph::search_nodes(
            &guard.simulation,
            &app_state.search_query,
            config.search.max_results,
        );
    }
    app_state.search_selected = 0;
}
