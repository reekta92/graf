mod app;
mod cli;
mod config;
mod graph;
mod linker;
mod ui;
mod util;

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

use crate::graph::input::GraphAction;

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
                    config.filter.max_nodes,
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
                && let Some(action) = graph::input::handle_graph_keys(graph_state, key, config)
            {
                match action {
                    GraphAction::Quit => return Ok(Some(EventAction::Quit)),
                    GraphAction::ToggleHelp => {
                        app_state.show_help = true;
                        return Ok(None);
                    }
                    GraphAction::ToggleSearch => {
                        app_state.search_active = true;
                        return Ok(None);
                    }
                    GraphAction::ToggleMinimap => {
                        app_state.show_minimap = !app_state.show_minimap;
                        return Ok(None);
                    }
                    GraphAction::ToggleLegend => {
                        app_state.show_legend = !app_state.show_legend;
                        return Ok(None);
                    }
                    GraphAction::ToggleGrid => {
                        app_state.show_grid = !app_state.show_grid;
                        return Ok(None);
                    }
                    GraphAction::ToggleStatus => {
                        app_state.show_status_bar = !app_state.show_status_bar;
                        return Ok(None);
                    }
                    GraphAction::OpenFile(path) => {
                        return Ok(Some(EventAction::OpenFile(path)));
                    }
                    GraphAction::Refresh => {
                        app_state.refresh_simulation(config);
                        return Ok(None);
                    }
                    GraphAction::ReloadConfig => {
                        return Ok(Some(EventAction::ReloadConfig));
                    }
                }
            }
            Ok(None)
        }
        Event::Mouse(mouse_event) => {
            if app_state.show_help || app_state.search_active {
                return Ok(None);
            }
            if let Some(graph_state) = &app_state.graph_state
                && let Some(action) = graph::input::handle_graph_mouse(
                    graph_state,
                    mouse_event,
                    frame_area(guard)?,
                    &mut app_state.graph_mouse_state,
                    config,
                )
                && let GraphAction::OpenFile(path) = action
            {
                return Ok(Some(EventAction::OpenFile(path)));
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn apply_cli_overrides(config: &mut config::GrafConfig, cli: &cli::Cli) {
    if let Some(ref theme) = cli.theme
        && let Ok(t) = theme.parse()
    {
        config.visual.theme = t;
    }
    if let Some(max) = cli.max_nodes {
        config.filter.max_nodes = max;
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
        config.editor.command = editor.clone();
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
        config.filter.max_nodes,
    );

    if files.is_empty() {
        eprintln!("No markdown files found in {}", cwd.display());
        std::process::exit(1);
    }

    let mut guard = TerminalGuard::new()?;
    let mut app_state = app::AppState::new(&config, files, config_errors);
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

    let editor = if !config.editor.command.is_empty() {
        config.editor.command.clone()
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
                    let guard = graph_state.read().unwrap_or_else(|e| e.into_inner());
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
                let mut guard = graph_state.write().unwrap_or_else(|e| e.into_inner());
                guard.selected_node = Some(idx);
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
        let guard = graph_state.read().unwrap_or_else(|e| e.into_inner());
        app_state.search_results = graph::search_nodes(
            &guard.simulation,
            &app_state.search_query,
            config.search.max_results,
        );
    }
    app_state.search_selected = 0;
}
