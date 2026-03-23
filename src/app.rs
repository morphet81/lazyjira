use crate::adf;
use crate::jira::{self, JiraProject, WorkItem, WorkItemDetail};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Projects,
    Tickets,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailMode {
    Viewing,
    Normal, // vi normal mode
    Insert, // vi insert mode
}

#[derive(Debug, Clone)]
pub enum SaveStatus {
    Saving,
    Saved,
    #[allow(dead_code)]
    Error(String),
}

/// A board column derived from status categories.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub items: Vec<WorkItem>,
}

#[derive(Debug, Clone)]
pub struct EditableField {
    pub label: &'static str,
    pub acli_flag: &'static str,
    pub value: String,
    pub original: String,
    pub multiline: bool,
}

pub struct App {
    pub projects: Vec<JiraProject>,
    pub project_index: usize,
    pub columns: Vec<Column>,
    pub column_index: usize,
    pub ticket_index: usize,
    pub detail: Option<WorkItemDetail>,
    pub active_pane: Pane,
    pub status_message: String,
    pub should_quit: bool,
    pub loading_tickets: bool,
    pub loading_detail: bool,
    // Detail editing state
    pub detail_mode: DetailMode,
    pub detail_field_index: usize,
    pub editable_fields: Vec<EditableField>,
    pub edit_cursor_col: usize,
    pub edit_cursor_row: usize,
    pub save_status: Option<SaveStatus>,
    pub last_tickets_refresh: Option<Instant>,
    pub pending_d: bool, // for vi "dd" command
}

impl App {
    pub fn new() -> Self {
        Self {
            projects: Vec::new(),
            project_index: 0,
            columns: Vec::new(),
            column_index: 0,
            ticket_index: 0,
            detail: None,
            active_pane: Pane::Projects,
            status_message: String::new(),
            should_quit: false,
            loading_tickets: false,
            loading_detail: false,
            detail_mode: DetailMode::Viewing,
            detail_field_index: 0,
            editable_fields: Vec::new(),
            edit_cursor_col: 0,
            edit_cursor_row: 0,
            save_status: None,
            last_tickets_refresh: None,
            pending_d: false,
        }
    }

    pub fn load_projects(&mut self) {
        self.status_message = "Loading projects...".to_string();
        match jira::fetch_projects() {
            Ok(projects) => {
                self.projects = projects;
                self.project_index = 0;
                self.status_message = format!("{} projects loaded", self.projects.len());
            }
            Err(e) => {
                self.status_message = format!("Error: {}", e);
            }
        }
    }

    pub fn load_workitems(&mut self) {
        if self.projects.is_empty() {
            return;
        }
        let project_key = &self.projects[self.project_index].key.clone();
        self.status_message = format!("Loading tickets for {}...", project_key);

        match jira::fetch_workitems(project_key) {
            Ok(items) => {
                self.build_columns(items);
                self.column_index = 0;
                self.ticket_index = 0;
                self.detail = None;
                self.editable_fields.clear();
                self.detail_mode = DetailMode::Viewing;
                self.last_tickets_refresh = Some(Instant::now());
                self.status_message = format!(
                    "{} tickets in {} columns",
                    self.columns.iter().map(|c| c.items.len()).sum::<usize>(),
                    self.columns.len()
                );
            }
            Err(e) => {
                self.columns.clear();
                self.status_message = format!("Error: {}", e);
            }
        }
    }

    /// Refresh work items while preserving current column and ticket selection.
    pub fn refresh_workitems(&mut self) {
        if self.projects.is_empty() || self.columns.is_empty() {
            return;
        }
        let project_key = self.projects[self.project_index].key.clone();
        let prev_column = self.column_index;
        let prev_ticket = self.ticket_index;

        match jira::fetch_workitems(&project_key) {
            Ok(items) => {
                self.build_columns(items);
                self.column_index = prev_column.min(self.columns.len().saturating_sub(1));
                let ticket_count = self.current_tickets().len();
                self.ticket_index = prev_ticket.min(ticket_count.saturating_sub(1));
                self.last_tickets_refresh = Some(Instant::now());
                self.status_message = format!(
                    "{} tickets in {} columns (refreshed)",
                    self.columns.iter().map(|c| c.items.len()).sum::<usize>(),
                    self.columns.len()
                );
            }
            Err(e) => {
                self.status_message = format!("Refresh error: {}", e);
            }
        }
    }

    pub fn needs_auto_refresh(&self) -> bool {
        if self.columns.is_empty() || self.is_editing() {
            return false;
        }
        match self.last_tickets_refresh {
            Some(last) => last.elapsed() >= AUTO_REFRESH_INTERVAL,
            None => false,
        }
    }

    fn build_columns(&mut self, items: Vec<WorkItem>) {
        let mut groups: BTreeMap<u32, (String, Vec<WorkItem>)> = BTreeMap::new();

        for item in items {
            let cat_id = item.fields.status.status_category.id;
            let cat_name = item.fields.status.status_category.name.clone();
            groups
                .entry(cat_id)
                .or_insert_with(|| (cat_name, Vec::new()))
                .1
                .push(item);
        }

        let order = |id: &u32| -> u32 {
            match id {
                2 => 0,
                4 => 1,
                3 => 2,
                other => 3 + other,
            }
        };

        let mut sorted_keys: Vec<u32> = groups.keys().cloned().collect();
        sorted_keys.sort_by_key(|k| order(k));

        self.columns = sorted_keys
            .into_iter()
            .map(|id| {
                let (name, items) = groups.remove(&id).unwrap();
                Column { name, items }
            })
            .collect();
    }

    pub fn load_detail(&mut self) {
        if let Some(item) = self.current_ticket() {
            let key = item.key.clone();
            self.status_message = format!("Loading {}...", key);
            match jira::fetch_workitem_detail(&key) {
                Ok(detail) => {
                    self.status_message = key;
                    self.populate_editable_fields(&detail);
                    self.detail = Some(detail);
                    self.detail_mode = DetailMode::Viewing;
                    self.detail_field_index = 0;
                    self.save_status = None;
                }
                Err(e) => {
                    self.status_message = format!("Error: {}", e);
                }
            }
        }
    }

    fn populate_editable_fields(&mut self, detail: &WorkItemDetail) {
        let summary = detail.fields.summary.clone();
        let description = detail
            .fields
            .description
            .as_ref()
            .map(|d| adf::render_adf(d))
            .unwrap_or_default();

        self.editable_fields = vec![
            EditableField {
                label: "Summary",
                acli_flag: "summary",
                original: summary.clone(),
                value: summary,
                multiline: false,
            },
            EditableField {
                label: "Description",
                acli_flag: "description",
                original: description.clone(),
                value: description,
                multiline: true,
            },
        ];
    }

    pub fn current_tickets(&self) -> &[WorkItem] {
        if self.columns.is_empty() {
            return &[];
        }
        &self.columns[self.column_index].items
    }

    pub fn current_ticket(&self) -> Option<&WorkItem> {
        let tickets = self.current_tickets();
        if tickets.is_empty() {
            None
        } else {
            Some(&tickets[self.ticket_index.min(tickets.len() - 1)])
        }
    }

    pub fn current_column_name(&self) -> &str {
        if self.columns.is_empty() {
            "No columns"
        } else {
            &self.columns[self.column_index].name
        }
    }

    pub fn is_editing(&self) -> bool {
        matches!(self.detail_mode, DetailMode::Normal | DetailMode::Insert)
    }

    pub fn is_insert_mode(&self) -> bool {
        self.detail_mode == DetailMode::Insert
    }

    /// Get field value as lines (preserving trailing empty lines).
    fn field_lines(value: &str) -> Vec<String> {
        if value.is_empty() {
            return vec![String::new()];
        }
        value.split('\n').map(String::from).collect()
    }

    fn current_field_lines(&self) -> Vec<String> {
        if self.editable_fields.is_empty() {
            return vec![String::new()];
        }
        Self::field_lines(&self.editable_fields[self.detail_field_index].value)
    }

    fn set_field_from_lines(&mut self, lines: &[String]) {
        if self.editable_fields.is_empty() {
            return;
        }
        self.editable_fields[self.detail_field_index].value = lines.join("\n");
    }

    fn clamp_cursor(&mut self) {
        let lines = self.current_field_lines();
        self.edit_cursor_row = self.edit_cursor_row.min(lines.len().saturating_sub(1));
        let line_len = lines[self.edit_cursor_row].len();
        if self.detail_mode == DetailMode::Normal {
            // In normal mode cursor sits ON a char, not past end
            self.edit_cursor_col = self.edit_cursor_col.min(line_len.saturating_sub(1).max(0));
        } else {
            self.edit_cursor_col = self.edit_cursor_col.min(line_len);
        }
    }

    // --- Input handling ---

    pub fn move_up(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                if self.project_index > 0 {
                    self.project_index -= 1;
                }
            }
            Pane::Tickets => {
                if self.ticket_index > 0 {
                    self.ticket_index -= 1;
                }
            }
            Pane::Detail => {
                if self.detail_field_index > 0 {
                    self.detail_field_index -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                if !self.projects.is_empty() && self.project_index < self.projects.len() - 1 {
                    self.project_index += 1;
                }
            }
            Pane::Tickets => {
                let len = self.current_tickets().len();
                if len > 0 && self.ticket_index < len - 1 {
                    self.ticket_index += 1;
                }
            }
            Pane::Detail => {
                if !self.editable_fields.is_empty()
                    && self.detail_field_index < self.editable_fields.len() - 1
                {
                    self.detail_field_index += 1;
                }
            }
        }
    }

    pub fn move_left(&mut self) {
        if self.active_pane == Pane::Tickets && !self.columns.is_empty() {
            if self.column_index == 0 {
                self.column_index = self.columns.len() - 1;
            } else {
                self.column_index -= 1;
            }
            self.ticket_index = 0;
            self.detail = None;
            self.editable_fields.clear();
        }
    }

    pub fn move_right(&mut self) {
        if self.active_pane == Pane::Tickets && !self.columns.is_empty() {
            self.column_index = (self.column_index + 1) % self.columns.len();
            self.ticket_index = 0;
            self.detail = None;
            self.editable_fields.clear();
        }
    }

    pub fn enter(&mut self) -> bool {
        match self.active_pane {
            Pane::Projects if !self.projects.is_empty() => {
                self.loading_tickets = true;
                self.columns.clear();
                self.detail = None;
                self.editable_fields.clear();
                self.active_pane = Pane::Tickets;
                true
            }
            Pane::Tickets if self.current_ticket().is_some() => {
                self.loading_detail = true;
                self.detail = None;
                self.editable_fields.clear();
                true
            }
            _ => false,
        }
    }

    pub fn perform_pending_load(&mut self) {
        if self.loading_tickets {
            self.load_workitems();
            self.loading_tickets = false;
        }
        if self.loading_detail {
            self.load_detail();
            self.loading_detail = false;
        }
    }

    pub fn toggle_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Projects => Pane::Tickets,
            Pane::Tickets => Pane::Detail,
            Pane::Detail => Pane::Projects,
        };
    }

    pub fn select_pane(&mut self, n: u8) {
        self.active_pane = match n {
            1 => Pane::Projects,
            2 => Pane::Tickets,
            3 => Pane::Detail,
            _ => return,
        };
    }

    // --- Edit mode ---

    pub fn start_editing(&mut self) {
        if self.active_pane != Pane::Detail || self.editable_fields.is_empty() {
            return;
        }
        self.detail_mode = DetailMode::Normal;
        self.save_status = None;
        self.pending_d = false;
        self.edit_cursor_row = 0;
        self.edit_cursor_col = 0;
    }

    /// Esc in Normal mode: save and return to Viewing.
    /// Returns true if save is needed.
    pub fn stop_editing(&mut self) -> bool {
        self.detail_mode = DetailMode::Viewing;
        self.pending_d = false;
        if self.editable_fields.is_empty() {
            return false;
        }
        let field = &self.editable_fields[self.detail_field_index];
        field.value != field.original
    }

    /// Esc in Insert mode: return to Normal mode.
    pub fn exit_insert_mode(&mut self) {
        self.detail_mode = DetailMode::Normal;
        // In normal mode cursor can't be past last char
        self.clamp_cursor();
    }

    pub fn prepare_save(&mut self) {
        self.save_status = Some(SaveStatus::Saving);
    }

    pub fn perform_save(&mut self) {
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        let field = &self.editable_fields[self.detail_field_index];
        let value = field.value.clone();
        let flag = field.acli_flag;

        match jira::update_workitem(&key, flag, &value) {
            Ok(()) => {
                self.editable_fields[self.detail_field_index].original = value;
                self.save_status = Some(SaveStatus::Saved);
            }
            Err(e) => {
                self.save_status = Some(SaveStatus::Error(e.to_string()));
            }
        }
    }

    // --- Vi normal mode commands ---

    pub fn vi_h(&mut self) {
        if self.edit_cursor_col > 0 {
            self.edit_cursor_col -= 1;
        }
    }

    pub fn vi_l(&mut self) {
        let lines = self.current_field_lines();
        let line_len = lines[self.edit_cursor_row].len();
        if line_len > 0 && self.edit_cursor_col < line_len - 1 {
            self.edit_cursor_col += 1;
        }
    }

    pub fn vi_j(&mut self) {
        let field = &self.editable_fields[self.detail_field_index];
        if !field.multiline {
            return;
        }
        let lines = self.current_field_lines();
        if self.edit_cursor_row + 1 < lines.len() {
            self.edit_cursor_row += 1;
            self.clamp_cursor();
        }
    }

    pub fn vi_k(&mut self) {
        if self.edit_cursor_row > 0 {
            self.edit_cursor_row -= 1;
            self.clamp_cursor();
        }
    }

    pub fn vi_0(&mut self) {
        self.edit_cursor_col = 0;
    }

    pub fn vi_dollar(&mut self) {
        let lines = self.current_field_lines();
        let line_len = lines[self.edit_cursor_row].len();
        self.edit_cursor_col = line_len.saturating_sub(1);
    }

    pub fn vi_w(&mut self) {
        let lines = self.current_field_lines();
        let line = &lines[self.edit_cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.edit_cursor_col;
        // Skip current word
        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }
        // Skip whitespace
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        if col >= chars.len() {
            self.edit_cursor_col = chars.len().saturating_sub(1);
        } else {
            self.edit_cursor_col = col;
        }
    }

    pub fn vi_b(&mut self) {
        let lines = self.current_field_lines();
        let line = &lines[self.edit_cursor_row];
        let chars: Vec<char> = line.chars().collect();
        if self.edit_cursor_col == 0 {
            return;
        }
        let mut col = self.edit_cursor_col - 1;
        // Skip whitespace backwards
        while col > 0 && chars[col].is_whitespace() {
            col -= 1;
        }
        // Skip word backwards
        while col > 0 && !chars[col - 1].is_whitespace() {
            col -= 1;
        }
        self.edit_cursor_col = col;
    }

    pub fn vi_i(&mut self) {
        // Insert before cursor
        self.detail_mode = DetailMode::Insert;
    }

    pub fn vi_a(&mut self) {
        // Append after cursor
        let lines = self.current_field_lines();
        let line_len = lines[self.edit_cursor_row].len();
        if line_len > 0 {
            self.edit_cursor_col = (self.edit_cursor_col + 1).min(line_len);
        }
        self.detail_mode = DetailMode::Insert;
    }

    pub fn vi_shift_a(&mut self) {
        // Append at end of line
        let lines = self.current_field_lines();
        self.edit_cursor_col = lines[self.edit_cursor_row].len();
        self.detail_mode = DetailMode::Insert;
    }

    pub fn vi_shift_i(&mut self) {
        // Insert at beginning of line
        self.edit_cursor_col = 0;
        self.detail_mode = DetailMode::Insert;
    }

    pub fn vi_o(&mut self) {
        // Open line below
        let field = &self.editable_fields[self.detail_field_index];
        if !field.multiline {
            return;
        }
        let mut lines = self.current_field_lines();
        self.edit_cursor_row += 1;
        lines.insert(self.edit_cursor_row, String::new());
        self.edit_cursor_col = 0;
        self.set_field_from_lines(&lines);
        self.detail_mode = DetailMode::Insert;
    }

    pub fn vi_shift_o(&mut self) {
        // Open line above
        let field = &self.editable_fields[self.detail_field_index];
        if !field.multiline {
            return;
        }
        let mut lines = self.current_field_lines();
        lines.insert(self.edit_cursor_row, String::new());
        self.edit_cursor_col = 0;
        self.set_field_from_lines(&lines);
        self.detail_mode = DetailMode::Insert;
    }

    pub fn vi_x(&mut self) {
        // Delete char under cursor
        let mut lines = self.current_field_lines();
        let line = &mut lines[self.edit_cursor_row];
        if !line.is_empty() && self.edit_cursor_col < line.len() {
            line.remove(self.edit_cursor_col);
            self.set_field_from_lines(&lines);
            self.clamp_cursor();
        }
    }

    pub fn vi_dd(&mut self) {
        let field = &self.editable_fields[self.detail_field_index];
        if !field.multiline {
            // Single-line: clear content
            self.editable_fields[self.detail_field_index].value.clear();
            self.edit_cursor_col = 0;
            return;
        }
        let mut lines = self.current_field_lines();
        if lines.len() <= 1 {
            lines[0].clear();
        } else {
            lines.remove(self.edit_cursor_row);
        }
        self.set_field_from_lines(&lines);
        self.clamp_cursor();
    }

    pub fn vi_shift_d(&mut self) {
        // Delete from cursor to end of line
        let mut lines = self.current_field_lines();
        let line = &mut lines[self.edit_cursor_row];
        line.truncate(self.edit_cursor_col);
        self.set_field_from_lines(&lines);
        self.clamp_cursor();
    }

    pub fn vi_gg(&mut self) {
        self.edit_cursor_row = 0;
        self.clamp_cursor();
    }

    pub fn vi_shift_g(&mut self) {
        let lines = self.current_field_lines();
        self.edit_cursor_row = lines.len().saturating_sub(1);
        self.clamp_cursor();
    }

    // --- Insert mode handlers ---

    pub fn insert_char(&mut self, c: char) {
        if self.editable_fields.is_empty() {
            return;
        }
        let mut lines = self.current_field_lines();
        let line = &mut lines[self.edit_cursor_row];
        let col = self.edit_cursor_col.min(line.len());
        line.insert(col, c);
        self.edit_cursor_col = col + 1;
        self.set_field_from_lines(&lines);
    }

    pub fn insert_backspace(&mut self) {
        if self.editable_fields.is_empty() {
            return;
        }
        let mut lines = self.current_field_lines();
        if self.edit_cursor_col > 0 {
            let line = &mut lines[self.edit_cursor_row];
            let col = self.edit_cursor_col.min(line.len());
            line.remove(col - 1);
            self.edit_cursor_col = col - 1;
        } else if self.edit_cursor_row > 0 {
            let current = lines.remove(self.edit_cursor_row);
            self.edit_cursor_row -= 1;
            self.edit_cursor_col = lines[self.edit_cursor_row].len();
            lines[self.edit_cursor_row].push_str(&current);
        }
        self.set_field_from_lines(&lines);
    }

    pub fn insert_enter(&mut self) {
        if self.editable_fields.is_empty() {
            return;
        }
        let field = &self.editable_fields[self.detail_field_index];
        if !field.multiline {
            return;
        }
        let mut lines = self.current_field_lines();
        let col = self.edit_cursor_col.min(lines[self.edit_cursor_row].len());
        let rest = lines[self.edit_cursor_row].split_off(col);
        self.edit_cursor_row += 1;
        lines.insert(self.edit_cursor_row, rest);
        self.edit_cursor_col = 0;
        self.set_field_from_lines(&lines);
    }

    pub fn insert_move_left(&mut self) {
        if self.edit_cursor_col > 0 {
            self.edit_cursor_col -= 1;
        }
    }

    pub fn insert_move_right(&mut self) {
        let lines = self.current_field_lines();
        let line_len = lines[self.edit_cursor_row].len();
        if self.edit_cursor_col < line_len {
            self.edit_cursor_col += 1;
        }
    }

    pub fn insert_move_up(&mut self) {
        if self.edit_cursor_row > 0 {
            self.edit_cursor_row -= 1;
            let lines = self.current_field_lines();
            let line_len = lines[self.edit_cursor_row].len();
            self.edit_cursor_col = self.edit_cursor_col.min(line_len);
        }
    }

    pub fn insert_move_down(&mut self) {
        let field = &self.editable_fields[self.detail_field_index];
        if !field.multiline {
            return;
        }
        let lines = self.current_field_lines();
        if self.edit_cursor_row + 1 < lines.len() {
            self.edit_cursor_row += 1;
            let line_len = lines[self.edit_cursor_row].len();
            self.edit_cursor_col = self.edit_cursor_col.min(line_len);
        }
    }
}
