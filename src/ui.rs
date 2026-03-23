use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Pane, SaveStatus};

const LEFT_WIDTH: u16 = 60;

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(LEFT_WIDTH), Constraint::Min(1)])
        .split(size);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
        .split(main_chunks[0]);

    draw_projects(frame, app, left_chunks[0]);
    draw_tickets(frame, app, left_chunks[1]);
    draw_detail(frame, app, main_chunks[1]);

    if app.show_epic_popup {
        draw_epic_popup(frame, app, left_chunks[1]);
    }
}

fn focused_border_style(app: &App, pane: Pane) -> Style {
    if app.active_pane == pane {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn draw_projects(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [1] Projects ")
        .borders(Borders::ALL)
        .border_style(focused_border_style(app, Pane::Projects));

    if app.loading_projects {
        let loading = Paragraph::new("Loading projects...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    &p.key,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::raw(&p.name),
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
    if !app.projects.is_empty() {
        state.select(Some(app.project_index));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_tickets(frame: &mut Frame, app: &App, area: Rect) {
    let epic_label = app
        .selected_epic
        .as_deref()
        .unwrap_or("All");
    let title = if app.columns.is_empty() {
        " [2] Tickets ".to_string()
    } else {
        format!(
            " [2] ◀ {} ▶  ({}/{}) [{}] [{}] ",
            app.current_column_name(),
            app.column_index + 1,
            app.columns.len(),
            app.ticket_sort.label(),
            epic_label,
        )
    };

    let border_style = focused_border_style(app, Pane::Tickets);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    if app.loading_tickets {
        let loading = Paragraph::new("Loading tickets...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    // Split: ticket list + hint bar
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let list_area = chunks[0];
    let hint_area = chunks[1];

    // Hint bar
    if app.active_pane == Pane::Tickets {
        let hint = Paragraph::new(" ◀▶ columns | ↑↓ select | e epics | s start | S-↑↓ sort | P priority | r refresh")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, hint_area);
    }

    // Ticket list
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
    frame.render_stateful_widget(list, list_area, &mut state);
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    // Build title with save status on the right
    let title_left = " [3] Detail ";
    let title = build_detail_title(title_left, &app.save_status, area.width);

    let border_style = if app.is_editing() {
        Style::default().fg(Color::Yellow)
    } else {
        focused_border_style(app, Pane::Detail)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    // Loading state
    if app.loading_detail {
        let loading = Paragraph::new("Loading details...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, area);
        return;
    }

    // No detail loaded
    let detail = match &app.detail {
        Some(d) => d,
        None => {
            let p = Paragraph::new(app.status_message.as_str())
                .block(block)
                .wrap(Wrap { trim: false });
            frame.render_widget(p, area);
            return;
        }
    };

    // Split: content area + hint bar (1 line)
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let content_area = chunks[0];
    let hint_area = chunks[1];

    // --- Hint bar ---
    let hints = if app.is_insert_mode() {
        " -- INSERT --  Esc → normal"
    } else if app.is_editing() {
        " -- NORMAL --  i insert | a append | o open line | dd del line | x del char | Esc save+quit"
    } else if app.active_pane == Pane::Detail {
        " ↑↓ select | e edit | q quit"
    } else {
        ""
    };
    let hint_line = Paragraph::new(hints).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint_line, hint_area);

    // --- Content: render manually into sub-rects ---
    // We need precise control for bordered field frames and cursor placement.
    let is_detail_focused = app.active_pane == Pane::Detail;

    // First, render the non-editable header as a Paragraph in the top portion
    let mut header_lines: Vec<Line> = Vec::new();
    header_lines.push(Line::from(Span::styled(
        &detail.key,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    let mut meta_parts: Vec<String> = Vec::new();
    if let Some(ref it) = detail.fields.issuetype {
        meta_parts.push(format!("Type: {}", it.name));
    }
    meta_parts.push(format!("Status: {}", detail.fields.status.name));
    if let Some(ref p) = detail.fields.priority {
        meta_parts.push(format!("Priority: {}", p.name));
    }
    header_lines.push(Line::from(meta_parts.join("  ")));

    let mut meta2: Vec<String> = Vec::new();
    if let Some(ref a) = detail.fields.assignee {
        meta2.push(format!("Assignee: {}", a.display_name));
    }
    if let Some(ref created) = detail.fields.created {
        meta2.push(format!("Created: {}", &created[..created.len().min(10)]));
    }
    if let Some(ref updated) = detail.fields.updated {
        meta2.push(format!("Updated: {}", &updated[..updated.len().min(10)]));
    }
    if !meta2.is_empty() {
        header_lines.push(Line::from(meta2.join("  ")));
    }

    let header_height = header_lines.len() as u16;
    if content_area.height <= header_height + 1 {
        let p = Paragraph::new(header_lines);
        frame.render_widget(p, content_area);
        return;
    }

    // Split content_area: header | rest for fields
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(1),
        ])
        .split(content_area);

    let header_p = Paragraph::new(header_lines);
    frame.render_widget(header_p, content_chunks[0]);

    // Now render editable fields and comments in the remaining area
    let fields_area = content_chunks[1];
    let mut y_offset: u16 = 0;

    for (i, field) in app.editable_fields.iter().enumerate() {
        let is_selected = is_detail_focused && i == app.detail_field_index;
        let is_editing_this = is_selected && app.is_editing();

        // Field label (1 line)
        if y_offset >= fields_area.height {
            break;
        }
        let label_area = Rect::new(
            fields_area.x,
            fields_area.y + y_offset,
            fields_area.width,
            1,
        );
        let marker = if is_selected { "▸ " } else { "  " };
        let label_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let label_line = Paragraph::new(Line::from(vec![
            Span::raw(marker),
            Span::styled(field.label, label_style),
        ]));
        frame.render_widget(label_line, label_area);
        y_offset += 1;

        // Field value in bordered frame
        let value_lines: Vec<&str> = if field.value.is_empty() {
            vec![""]
        } else {
            field.value.split('\n').collect()
        };
        // Frame height: value lines + 2 (borders), capped to remaining space
        let frame_height = (value_lines.len() as u16 + 2).min(fields_area.height.saturating_sub(y_offset));
        if frame_height < 3 {
            // Not enough space for bordered frame
            y_offset += 1;
            continue;
        }

        let value_area = Rect::new(
            fields_area.x + 1, // indent slightly
            fields_area.y + y_offset,
            fields_area.width.saturating_sub(2),
            frame_height,
        );

        let field_border_style = if is_editing_this {
            Style::default().fg(Color::Yellow)
        } else if is_selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let field_block = Block::default()
            .borders(Borders::ALL)
            .border_style(field_border_style);

        let field_inner = field_block.inner(value_area);
        frame.render_widget(field_block, value_area);

        // Render value text inside the frame
        let value_text: Vec<Line> = value_lines
            .iter()
            .map(|l| Line::from(l.to_string()))
            .collect();
        let value_p = Paragraph::new(value_text)
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(value_p, field_inner);

        // Set cursor if editing this field
        if is_editing_this {
            let cursor_row = app.edit_cursor_row.min(value_lines.len().saturating_sub(1));
            let line_len = value_lines[cursor_row].len();
            let cursor_col = if app.is_insert_mode() {
                app.edit_cursor_col.min(line_len)
            } else {
                app.edit_cursor_col.min(line_len.saturating_sub(1).max(0))
            };
            let cx = field_inner.x + cursor_col as u16;
            let cy = field_inner.y + cursor_row as u16;
            if cy < field_inner.y + field_inner.height && cx < field_inner.x + field_inner.width {
                frame.set_cursor_position(Position::new(cx, cy));
            }
        }

        y_offset += frame_height;

        // Small gap between fields
        y_offset += 1;
    }

    // Comments section
    if let Some(ref comments) = detail.fields.comment {
        let rendered = crate::adf::render_comments(comments);
        if !rendered.is_empty() && y_offset < fields_area.height {
            let remaining = fields_area.height.saturating_sub(y_offset);
            let comments_area = Rect::new(
                fields_area.x,
                fields_area.y + y_offset,
                fields_area.width,
                remaining,
            );
            let sep_width = comments_area.width as usize;
            let mut comment_lines: Vec<Line> = Vec::new();
            comment_lines.push(Line::from(Span::styled(
                "─".repeat(sep_width),
                Style::default().fg(Color::DarkGray),
            )));
            comment_lines.push(Line::from(Span::styled(
                "Comments",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            for l in rendered.lines() {
                comment_lines.push(Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::Gray),
                )));
            }
            let cp = Paragraph::new(comment_lines).wrap(Wrap { trim: false });
            frame.render_widget(cp, comments_area);
        }
    }
}

fn build_detail_title<'a>(
    left: &'a str,
    save_status: &Option<SaveStatus>,
    width: u16,
) -> Line<'a> {
    match save_status {
        None => Line::from(left),
        Some(status) => {
            let (text, color) = match status {
                SaveStatus::Saving => ("Saving...", Color::Yellow),
                SaveStatus::Saved => ("Saved!", Color::Green),
                SaveStatus::Error(_) => ("Error saving details", Color::Red),
            };
            let right = format!(" {} ", text);
            let padding = width
                .saturating_sub(left.len() as u16)
                .saturating_sub(right.len() as u16);
            Line::from(vec![
                Span::raw(left),
                Span::raw(" ".repeat(padding as usize)),
                Span::styled(right, Style::default().fg(color)),
            ])
        }
    }
}

fn draw_epic_popup(frame: &mut Frame, app: &App, pane2_area: Rect) {
    // Centered within pane 2
    let popup_width = pane2_area.width.saturating_sub(4).min(50);
    let popup_height = (app.epics.len() as u16 + 3).min(20).min(pane2_area.height.saturating_sub(2));
    let x = pane2_area.x + (pane2_area.width.saturating_sub(popup_width)) / 2;
    let y = pane2_area.y + (pane2_area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Epic Filter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    if app.loading_epics {
        let loading = Paragraph::new("Loading epics...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading, popup_area);
        return;
    }

    // Build items: "All epics" + each epic
    let mut items: Vec<ListItem> = Vec::new();
    items.push(ListItem::new(Line::from(Span::styled(
        "All epics",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    ))));

    for epic in &app.epics {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(&epic.key, Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::raw(truncate(
                &epic.fields.summary,
                popup_width.saturating_sub(epic.key.len() as u16 + 5) as usize,
            )),
        ])));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    state.select(Some(app.epic_popup_index));
    frame.render_stateful_widget(list, popup_area, &mut state);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
