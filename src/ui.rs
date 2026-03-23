use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::adf;
use crate::app::{App, Pane};

const LEFT_WIDTH: u16 = 60;

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Main horizontal split: left (60 chars) | right (rest)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_WIDTH), Constraint::Min(1)])
        .split(size);

    // Left vertical split: top (projects) | bottom (tickets)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    draw_projects(frame, app, left_chunks[0]);
    draw_tickets(frame, app, left_chunks[1]);
    draw_detail(frame, app, main_chunks[1]);
}

fn focused_border_style(app: &App, pane: Pane) -> Style {
    if app.active_pane == pane {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn draw_projects(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(&p.key, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::raw(&p.name),
            ]))
        })
        .collect();

    let block = Block::default()
        .title(" [1] Projects ")
        .borders(Borders::ALL)
        .border_style(focused_border_style(app, Pane::Projects));

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !app.projects.is_empty() {
        state.select(Some(app.project_index));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_tickets(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let title = if app.columns.is_empty() {
        " [2] Tickets ".to_string()
    } else {
        format!(
            " [2] ◀ {} ▶  ({}/{}) ",
            app.current_column_name(),
            app.column_index + 1,
            app.columns.len()
        )
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(focused_border_style(app, Pane::Tickets));

    if app.loading_tickets {
        let loading = Paragraph::new("Loading tickets...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    let tickets = app.current_tickets();
    let items: Vec<ListItem> = tickets
        .iter()
        .map(|t| {
            let assignee = t
                .fields
                .assignee
                .as_ref()
                .map(|a| a.display_name.as_str())
                .unwrap_or("Unassigned");
            ListItem::new(Line::from(vec![
                Span::styled(&t.key, Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::raw(truncate(&t.fields.summary, 30)),
                Span::styled(
                    format!(" [{}]", assignee),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !tickets.is_empty() {
        state.select(Some(app.ticket_index));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let content = if app.loading_detail {
        "Loading detail...".to_string()
    } else if let Some(detail) = &app.detail {
        let mut text = String::new();

        // Header
        text.push_str(&detail.key);
        text.push('\n');

        if let Some(ref it) = detail.fields.issuetype {
            text.push_str(&format!("Type: {}\n", it.name));
        }
        text.push_str(&format!("Status: {}\n", detail.fields.status.name));
        if let Some(ref p) = detail.fields.priority {
            text.push_str(&format!("Priority: {}\n", p.name));
        }
        if let Some(ref a) = detail.fields.assignee {
            text.push_str(&format!("Assignee: {}\n", a.display_name));
        }
        if let Some(ref created) = detail.fields.created {
            text.push_str(&format!("Created: {}\n", &created[..created.len().min(10)]));
        }
        if let Some(ref updated) = detail.fields.updated {
            text.push_str(&format!("Updated: {}\n", &updated[..updated.len().min(10)]));
        }

        // Summary
        text.push_str(&format!("\n{}\n", detail.fields.summary));

        // Description
        if let Some(ref desc) = detail.fields.description {
            text.push_str("\n--- Description ---\n");
            text.push_str(&adf::render_adf(desc));
        }

        // Comments
        if let Some(ref comments) = detail.fields.comment {
            let rendered = adf::render_comments(comments);
            if !rendered.is_empty() {
                text.push_str("\n--- Comments ---\n");
                text.push_str(&rendered);
            }
        }

        text
    } else {
        app.status_message.clone()
    };

    let block = Block::default()
        .title(" [3] Detail ")
        .borders(Borders::ALL)
        .border_style(focused_border_style(app, Pane::Detail));

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
