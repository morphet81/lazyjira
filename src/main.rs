mod adf;
mod app;
mod config;
mod jira;
mod ui;
mod worktree;

use std::io;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{debug, info};
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
use std::fs::File;
use std::time::Duration;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() -> Result<()> {
    // Initialize file logger — stdout is unavailable (raw mode + alternate screen)
    let log_level = match std::env::var("LAZYJIRA_LOG").as_deref() {
        Ok("trace") => LevelFilter::Trace,
        Ok("debug") => LevelFilter::Debug,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        Ok("off") => LevelFilter::Off,
        _ => LevelFilter::Info,
    };
    if log_level != LevelFilter::Off {
        if let Ok(file) = File::create("/tmp/lazyjira.log") {
            let _ = WriteLogger::init(
                log_level,
                ConfigBuilder::new().set_time_format_rfc3339().build(),
                file,
            );
        }
    }

    info!("lazyjira starting");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    app.load_projects();

    let mut projects_loaded = false;

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Poll with short timeout so background tasks stay responsive
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // q always quits, regardless of loading state
                if key.code == KeyCode::Char('q') && !app.is_editing() && !app.is_insert_mode() {
                    app.should_quit = true;
                } else if app.start_popup.is_some() {
                    match &app.start_popup.as_ref().unwrap().phase {
                        app::StartPopupPhase::ChoosingType { .. } => {
                            match key.code {
                                KeyCode::Up => app.start_popup_up(),
                                KeyCode::Down => app.start_popup_down(),
                                KeyCode::Enter => {
                                    app.start_popup_confirm();
                                    app.run_start_ticket();
                                }
                                KeyCode::Esc => app.close_start_popup(),
                                _ => {}
                            }
                        }
                        app::StartPopupPhase::Creating { .. } => {
                            // In progress — ignore keys
                        }
                        app::StartPopupPhase::Done { .. } => {
                            // Any key dismisses — extract info before closing
                            let (was_ok, ticket_key, worktree_path) = match &app.start_popup.as_ref().unwrap().phase {
                                app::StartPopupPhase::Done { result: Ok(path) } => {
                                    let key = app.start_popup.as_ref().unwrap().ticket_key.clone();
                                    (true, key, Some(path.clone()))
                                }
                                _ => (false, String::new(), None),
                            };
                            info!("StartPopupPhase::Done — was_ok={}, ticket_key={:?}", was_ok, ticket_key);
                            info!("  worktree_path={:?}, zellij_tab={}", worktree_path, app.config.zellij_tab);
                            app.close_start_popup();
                            if was_ok {
                                app.refresh_workitems();
                                // Open Zellij tab if enabled and running inside Zellij
                                let inside_zellij = worktree::is_inside_zellij();
                                info!("  inside_zellij={}", inside_zellij);
                                if let (Some(path), true) = (worktree_path, app.config.zellij_tab && inside_zellij) {
                                    info!("Opening Zellij tab for {} at {}", ticket_key, path);
                                    worktree::open_zellij_tab(&ticket_key, &path);
                                }
                            }
                        }
                    }
                } else if app.show_epic_popup {
                    match key.code {
                        KeyCode::Up => app.epic_popup_up(),
                        KeyCode::Down => app.epic_popup_down(),
                        KeyCode::Enter => {
                            if app.select_epic() {
                                app.load_workitems();
                            }
                        }
                        KeyCode::Esc => app.close_epic_popup(),
                        _ => {}
                    }
                } else if app.is_insert_mode() {
                    // Vi INSERT mode
                    match key.code {
                        KeyCode::Esc => app.exit_insert_mode(),
                        KeyCode::Char(c) => app.insert_char(c),
                        KeyCode::Backspace => app.insert_backspace(),
                        KeyCode::Enter => app.insert_enter(),
                        KeyCode::Left => app.insert_move_left(),
                        KeyCode::Right => app.insert_move_right(),
                        KeyCode::Up => app.insert_move_up(),
                        KeyCode::Down => app.insert_move_down(),
                        _ => {}
                    }
                } else if app.is_editing() {
                    // Vi NORMAL mode
                    match key.code {
                        KeyCode::Esc => {
                            let changed = app.stop_editing();
                            if changed {
                                app.prepare_save();
                                terminal.draw(|f| ui::draw(f, &app))?;
                                app.perform_save();
                            }
                        }
                        KeyCode::Char('h') | KeyCode::Left => app.vi_h(),
                        KeyCode::Char('l') | KeyCode::Right => app.vi_l(),
                        KeyCode::Char('j') | KeyCode::Down => app.vi_j(),
                        KeyCode::Char('k') | KeyCode::Up => app.vi_k(),
                        KeyCode::Char('0') => app.vi_0(),
                        KeyCode::Char('$') => app.vi_dollar(),
                        KeyCode::Char('w') => app.vi_w(),
                        KeyCode::Char('b') => app.vi_b(),
                        KeyCode::Char('i') => app.vi_i(),
                        KeyCode::Char('a') => app.vi_a(),
                        KeyCode::Char('A') => app.vi_shift_a(),
                        KeyCode::Char('I') => app.vi_shift_i(),
                        KeyCode::Char('o') => app.vi_o(),
                        KeyCode::Char('O') => app.vi_shift_o(),
                        KeyCode::Char('x') => app.vi_x(),
                        KeyCode::Char('D') => app.vi_shift_d(),
                        KeyCode::Char('G') => app.vi_shift_g(),
                        KeyCode::Char('d') => {
                            if app.pending_d {
                                app.vi_dd();
                                app.pending_d = false;
                            } else {
                                app.pending_d = true;
                            }
                        }
                        KeyCode::Char('g') => {
                            // gg — we treat single 'g' as gg for simplicity
                            app.vi_gg();
                        }
                        _ => {
                            app.pending_d = false;
                        }
                    }
                    // Reset pending_d on non-d keys (except when just set)
                    if key.code != KeyCode::Char('d') {
                        app.pending_d = false;
                    }
                } else {
                    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                    match key.code {
                        KeyCode::Char('q') => {} // handled above
                        KeyCode::Up if shift => {
                            app.set_ticket_sort(app::TicketSort::KeyAsc);
                        }
                        KeyCode::Down if shift => {
                            app.set_ticket_sort(app::TicketSort::KeyDesc);
                        }
                        KeyCode::Char('P') => {
                            app.set_ticket_sort(app::TicketSort::Priority);
                        }
                        KeyCode::Up => app.move_up(),
                        KeyCode::Down => app.move_down(),
                        KeyCode::Left => app.move_left(),
                        KeyCode::Right => app.move_right(),
                        KeyCode::Enter => {
                            if app.enter() {
                                app.perform_pending_load();
                            }
                        }
                        KeyCode::Tab => app.toggle_pane(),
                        KeyCode::Char('1') => app.select_pane(1),
                        KeyCode::Char('2') => app.select_pane(2),
                        KeyCode::Char('3') => app.select_pane(3),
                        KeyCode::Char('e') => {
                            if app.active_pane == app::Pane::Tickets {
                                app.open_epic_popup();
                            } else {
                                app.start_editing();
                            }
                        }
                        KeyCode::Char('r') => {
                            match app.active_pane {
                                app::Pane::Projects => app.load_projects(),
                                app::Pane::Tickets => app.refresh_workitems(),
                                app::Pane::Detail => app.refresh_detail(),
                            }
                        }
                        KeyCode::Char('C') => {
                            match config::LazyJiraConfig::create_default() {
                                Ok(true) => {
                                    app.config = config::LazyJiraConfig::load();
                                    app.status_message = "Created .lazyjira config file".to_string();
                                }
                                Ok(false) => {
                                    app.status_message = ".lazyjira config already exists".to_string();
                                }
                                Err(e) => {
                                    app.status_message = format!("Error creating config: {}", e);
                                }
                            }
                        }
                        KeyCode::Char('s') => {
                            if app.start_current_ticket() {
                                app.open_start_popup();
                                // For bugs, auto-start (no type selection needed)
                                if matches!(
                                    app.start_popup.as_ref().map(|p| &p.phase),
                                    Some(app::StartPopupPhase::Creating { .. })
                                ) {
                                    app.run_start_ticket();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Poll all background tasks
        let had_projects = !app.projects.is_empty();
        app.poll_projects();
        app.poll_tickets();
        app.poll_details();
        app.poll_epics();
        app.poll_start_ticket();

        // Once projects finish loading for the first time, load tickets
        if !projects_loaded && !app.projects.is_empty() && !had_projects {
            projects_loaded = true;
            app.load_workitems();
        }

        // Auto-refresh every 5 minutes
        if app.needs_auto_refresh() {
            app.refresh_workitems();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
