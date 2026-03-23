mod adf;
mod app;
mod jira;
mod ui;

use std::io;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::time::Duration;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() -> Result<()> {
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

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Poll with 1s timeout so we can check auto-refresh
        if event::poll(Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.is_insert_mode() {
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
                    match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Up => app.move_up(),
                        KeyCode::Down => app.move_down(),
                        KeyCode::Left => app.move_left(),
                        KeyCode::Right => app.move_right(),
                        KeyCode::Enter => {
                            if app.enter() {
                                terminal.draw(|f| ui::draw(f, &app))?;
                                app.perform_pending_load();
                            }
                        }
                        KeyCode::Tab => app.toggle_pane(),
                        KeyCode::Char('1') => app.select_pane(1),
                        KeyCode::Char('2') => app.select_pane(2),
                        KeyCode::Char('3') => app.select_pane(3),
                        KeyCode::Char('e') => app.start_editing(),
                        KeyCode::Char('r') => {
                            if app.active_pane == app::Pane::Tickets {
                                app.loading_tickets = true;
                                terminal.draw(|f| ui::draw(f, &app))?;
                                app.refresh_workitems();
                                app.loading_tickets = false;
                            }
                        }
                        _ => {}
                    }
                }
            }
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
