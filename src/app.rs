use crate::adf;
use crate::config::LazyJiraConfig;
use crate::jira::{self, JiraProject, WorkItem, WorkItemDetail};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const MAX_DETAIL_FETCHES: usize = 5;

struct DetailFetch {
    key: String,
    receiver: mpsc::Receiver<anyhow::Result<WorkItemDetail>>,
    #[allow(dead_code)]
    handle: thread::JoinHandle<()>,
}

/// Compare Jira issue keys naturally: "NERO-2" < "NERO-10".
fn cmp_issue_key(a: &str, b: &str) -> Ordering {
    let (a_prefix, a_num) = split_key(a);
    let (b_prefix, b_num) = split_key(b);
    a_prefix.cmp(&b_prefix).then(a_num.cmp(&b_num))
}

fn split_key(key: &str) -> (&str, u64) {
    match key.rfind('-') {
        Some(pos) => {
            let prefix = &key[..pos];
            let num = key[pos + 1..].parse::<u64>().unwrap_or(0);
            (prefix, num)
        }
        None => (key, 0),
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketSort {
    Priority, // board rank order
    KeyAsc,
    KeyDesc,
}

impl TicketSort {
    pub fn label(&self) -> &'static str {
        match self {
            TicketSort::Priority => "priority",
            TicketSort::KeyAsc => "key ↑",
            TicketSort::KeyDesc => "key ↓",
        }
    }
}

/// A board column derived from status categories.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub items: Vec<WorkItem>,
    /// Items in original board rank order (for restoring priority sort).
    pub ranked_items: Vec<WorkItem>,
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
    pub loading_projects: bool,
    pub loading_tickets: bool,
    pub ticket_sort: TicketSort,
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
    // Epic filter
    pub show_epic_popup: bool,
    pub epics: Vec<WorkItem>,
    pub epic_popup_index: usize,
    pub selected_epic: Option<String>,
    pub loading_epics: bool,
    epics_receiver: Option<mpsc::Receiver<anyhow::Result<Vec<WorkItem>>>>,
    // Background detail fetching
    detail_queue: VecDeque<DetailFetch>,
    detail_cache: HashMap<String, WorkItemDetail>,
    // Background project/ticket loading
    projects_receiver: Option<mpsc::Receiver<anyhow::Result<Vec<JiraProject>>>>,
    tickets_receiver: Option<TicketsReceiver>,
    start_receiver: Option<mpsc::Receiver<Result<String, String>>>,
    // Config
    #[allow(dead_code)]
    pub config: LazyJiraConfig,
    // Start-ticket popup
    pub start_popup: Option<StartPopup>,
}

struct TicketsReceiver {
    rx: mpsc::Receiver<anyhow::Result<Vec<WorkItem>>>,
    is_refresh: bool,
    prev_column: usize,
    prev_ticket: usize,
}

/// Conventional commit types, ordered from most to least commonly used.
pub const COMMIT_TYPES: &[&str] = &[
    "feat", "fix", "refactor", "chore", "docs", "style", "test", "perf", "ci", "build", "revert",
];

pub struct StartPopup {
    pub ticket_key: String,
    pub phase: StartPopupPhase,
}

pub enum StartPopupPhase {
    /// User is choosing a conventional commit type.
    ChoosingType { selected: usize },
    /// Worktree creation is in progress.
    Creating { commit_type: String },
    /// Done — success or error.
    Done(Result<String, String>),
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
            loading_projects: false,
            loading_tickets: false,
            ticket_sort: TicketSort::Priority,
            loading_detail: false,
            detail_mode: DetailMode::Viewing,
            detail_field_index: 0,
            editable_fields: Vec::new(),
            edit_cursor_col: 0,
            edit_cursor_row: 0,
            save_status: None,
            last_tickets_refresh: None,
            pending_d: false,
            show_epic_popup: false,
            epics: Vec::new(),
            epic_popup_index: 0,
            selected_epic: None,
            loading_epics: false,
            epics_receiver: None,
            detail_queue: VecDeque::new(),
            detail_cache: HashMap::new(),
            projects_receiver: None,
            tickets_receiver: None,
            start_receiver: None,
            config: LazyJiraConfig::load(),
            start_popup: None,
        }
    }

    pub fn load_projects(&mut self) {
        self.loading_projects = true;
        let (tx, rx) = mpsc::channel();
        self.projects_receiver = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(jira::fetch_projects());
        });
    }

    pub fn poll_projects(&mut self) {
        let rx = match &self.projects_receiver {
            Some(rx) => rx,
            None => return,
        };
        match rx.try_recv() {
            Ok(Ok(projects)) => {
                self.projects = projects;
                self.project_index = 0;
                self.loading_projects = false;
                self.projects_receiver = None;
            }
            Ok(Err(e)) => {
                self.status_message = format!("Error: {}", e);
                self.loading_projects = false;
                self.projects_receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loading_projects = false;
                self.projects_receiver = None;
            }
        }
    }

    pub fn load_workitems(&mut self) {
        if self.projects.is_empty() {
            return;
        }
        let project_key = self.projects[self.project_index].key.clone();
        self.status_message = format!("Loading tickets for {}...", project_key);
        self.loading_tickets = true;
        let epic = self.selected_epic.clone();
        let (tx, rx) = mpsc::channel();
        self.tickets_receiver = Some(TicketsReceiver {
            rx,
            is_refresh: false,
            prev_column: 0,
            prev_ticket: 0,
        });
        thread::spawn(move || {
            let _ = tx.send(jira::fetch_workitems(&project_key, epic.as_deref()));
        });
    }

    fn handle_workitems_result(&mut self, items: Vec<WorkItem>, is_refresh: bool, prev_column: usize, prev_ticket: usize) {
        self.build_columns(items);
        if is_refresh {
            self.column_index = prev_column.min(self.columns.len().saturating_sub(1));
            let ticket_count = self.current_tickets().len();
            self.ticket_index = prev_ticket.min(ticket_count.saturating_sub(1));
            self.last_tickets_refresh = Some(Instant::now());
            self.status_message = format!(
                "{} tickets in {} columns (refreshed)",
                self.columns.iter().map(|c| c.items.len()).sum::<usize>(),
                self.columns.len()
            );
        } else {
            self.column_index = 0;
            self.ticket_index = 0;
            self.detail = None;
            self.editable_fields.clear();
            self.detail_mode = DetailMode::Viewing;
            self.detail_cache.clear();
            self.detail_queue.clear();
            self.loading_detail = false;
            self.active_pane = Pane::Tickets;
            self.last_tickets_refresh = Some(Instant::now());
            self.status_message = format!(
                "{} tickets in {} columns",
                self.columns.iter().map(|c| c.items.len()).sum::<usize>(),
                self.columns.len()
            );
            // Start background epic fetch if not already loaded
            let project_key = self.projects[self.project_index].key.clone();
            if self.epics.is_empty() && !self.loading_epics {
                self.start_epic_fetch(&project_key);
            }
            // Auto-fetch detail for first ticket
            self.request_current_detail();
        }
    }

    pub fn poll_tickets(&mut self) {
        let tr = match &self.tickets_receiver {
            Some(tr) => tr,
            None => return,
        };
        match tr.rx.try_recv() {
            Ok(Ok(items)) => {
                let is_refresh = tr.is_refresh;
                let prev_column = tr.prev_column;
                let prev_ticket = tr.prev_ticket;
                self.tickets_receiver = None;
                self.loading_tickets = false;
                self.handle_workitems_result(items, is_refresh, prev_column, prev_ticket);
            }
            Ok(Err(e)) => {
                let is_refresh = tr.is_refresh;
                self.tickets_receiver = None;
                self.loading_tickets = false;
                if is_refresh {
                    self.status_message = format!("Refresh error: {}", e);
                } else {
                    self.columns.clear();
                    self.status_message = format!("Error: {}", e);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loading_tickets = false;
                self.tickets_receiver = None;
            }
        }
    }

    fn start_epic_fetch(&mut self, project_key: &str) {
        self.loading_epics = true;
        let key = project_key.to_string();
        let (tx, rx) = mpsc::channel();
        self.epics_receiver = Some(rx);
        std::thread::spawn(move || {
            let result = jira::fetch_epics(&key);
            let _ = tx.send(result);
        });
    }

    /// Poll for background epic fetch completion. Call from the event loop.
    pub fn poll_epics(&mut self) {
        if let Some(rx) = &self.epics_receiver {
            match rx.try_recv() {
                Ok(Ok(mut epics)) => {
                    epics.sort_by(|a, b| cmp_issue_key(&a.key, &b.key));
                    self.epics = epics;
                    self.loading_epics = false;
                    self.epics_receiver = None;
                }
                Ok(Err(_)) => {
                    self.loading_epics = false;
                    self.epics_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still loading
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading_epics = false;
                    self.epics_receiver = None;
                }
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
        self.loading_tickets = true;
        let epic = self.selected_epic.clone();
        let (tx, rx) = mpsc::channel();
        self.tickets_receiver = Some(TicketsReceiver {
            rx,
            is_refresh: true,
            prev_column,
            prev_ticket,
        });
        thread::spawn(move || {
            let _ = tx.send(jira::fetch_workitems(&project_key, epic.as_deref()));
        });
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
            let is_epic = item
                .fields
                .issuetype
                .as_ref()
                .map(|t| t.name.eq_ignore_ascii_case("epic"))
                .unwrap_or(false);
            if is_epic {
                continue;
            }
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
                Column {
                    name,
                    ranked_items: items.clone(),
                    items,
                }
            })
            .collect();
        self.apply_sort();
    }

    pub fn set_ticket_sort(&mut self, sort: TicketSort) {
        self.ticket_sort = sort;
        self.apply_sort();
        self.ticket_index = 0;
    }

    fn apply_sort(&mut self) {
        for column in &mut self.columns {
            match self.ticket_sort {
                TicketSort::Priority => {
                    column.items = column.ranked_items.clone();
                }
                TicketSort::KeyAsc => {
                    column.items.sort_by(|a, b| cmp_issue_key(&a.key, &b.key));
                }
                TicketSort::KeyDesc => {
                    column.items.sort_by(|a, b| cmp_issue_key(&b.key, &a.key));
                }
            }
        }
    }

    pub fn load_detail(&mut self) {
        if let Some(item) = self.current_ticket() {
            let key = item.key.clone();
            self.show_detail_for_key(&key);
        }
    }

    /// Request detail for a key. Uses cache if available, otherwise queues a background fetch.
    fn show_detail_for_key(&mut self, key: &str) {
        // Check cache first
        if let Some(detail) = self.detail_cache.get(key).cloned() {
            self.populate_editable_fields(&detail);
            self.detail = Some(detail);
            self.detail_mode = DetailMode::Viewing;
            self.detail_field_index = 0;
            self.save_status = None;
            self.loading_detail = false;
            return;
        }

        // Already queued?
        if self.detail_queue.iter().any(|f| f.key == key) {
            self.loading_detail = true;
            return;
        }

        // Evict oldest if at capacity
        if self.detail_queue.len() >= MAX_DETAIL_FETCHES {
            self.detail_queue.pop_front(); // drops receiver + handle detaches
        }

        // Spawn background fetch
        let key_owned = key.to_string();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn({
            let k = key_owned.clone();
            move || {
                let result = jira::fetch_workitem_detail(&k);
                let _ = tx.send(result);
            }
        });

        self.detail_queue.push_back(DetailFetch {
            key: key_owned,
            receiver: rx,
            handle,
        });
        self.loading_detail = true;
    }

    /// Poll all background detail fetches. Call from the event loop.
    pub fn poll_details(&mut self) {
        let current_key = self.current_ticket().map(|t| t.key.clone());
        let mut completed = Vec::new();

        for (i, fetch) in self.detail_queue.iter().enumerate() {
            match fetch.receiver.try_recv() {
                Ok(Ok(detail)) => {
                    completed.push((i, fetch.key.clone(), Some(detail)));
                }
                Ok(Err(_)) => {
                    completed.push((i, fetch.key.clone(), None));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    completed.push((i, fetch.key.clone(), None));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        // Process completed in reverse order to preserve indices
        for (i, key, detail_opt) in completed.into_iter().rev() {
            self.detail_queue.remove(i);
            if let Some(detail) = detail_opt {
                self.detail_cache.insert(key.clone(), detail);

                // If this is the currently focused ticket, display it
                if current_key.as_deref() == Some(&key) {
                    if let Some(d) = self.detail_cache.get(&key).cloned() {
                        self.populate_editable_fields(&d);
                        self.detail = Some(d);
                        self.detail_mode = DetailMode::Viewing;
                        self.detail_field_index = 0;
                        self.save_status = None;
                        self.loading_detail = false;
                    }
                }
            }
        }

        // Update loading state for current ticket
        if let Some(ref ck) = current_key {
            if self.detail_cache.contains_key(ck) {
                self.loading_detail = false;
            } else if self.detail_queue.iter().any(|f| &f.key == ck) {
                self.loading_detail = true;
            }
        }
    }

    /// Trigger detail fetch for the currently focused ticket in pane 2.
    pub fn request_current_detail(&mut self) {
        if self.active_pane != Pane::Tickets {
            return;
        }
        if let Some(item) = self.current_ticket() {
            let key = item.key.clone();
            self.show_detail_for_key(&key);
        }
    }

    /// Assign to current user and transition to In Progress. Returns true if action was initiated.
    /// Only works from the "To Do" column.
    pub fn start_current_ticket(&mut self) -> bool {
        if self.active_pane != Pane::Tickets || self.columns.is_empty() {
            return false;
        }
        if self.current_column_name() != "To Do" {
            return false;
        }
        self.current_ticket().is_some()
    }

    /// Opens the start-ticket popup. For bugs, skips type selection.
    pub fn open_start_popup(&mut self) {
        let ticket = match self.current_ticket() {
            Some(t) => t.clone(),
            None => return,
        };
        let is_bug = ticket
            .fields
            .issuetype
            .as_ref()
            .map(|t| t.name.eq_ignore_ascii_case("bug"))
            .unwrap_or(false);

        let phase = if is_bug {
            StartPopupPhase::Creating { commit_type: "fix".to_string() }
        } else {
            StartPopupPhase::ChoosingType { selected: 0 }
        };

        self.start_popup = Some(StartPopup {
            ticket_key: ticket.key.clone(),
            phase,
        });
    }

    pub fn start_popup_up(&mut self) {
        if let Some(StartPopup { phase: StartPopupPhase::ChoosingType { selected }, .. }) = &mut self.start_popup {
            if *selected > 0 {
                *selected -= 1;
            }
        }
    }

    pub fn start_popup_down(&mut self) {
        if let Some(StartPopup { phase: StartPopupPhase::ChoosingType { selected }, .. }) = &mut self.start_popup {
            if *selected + 1 < COMMIT_TYPES.len() {
                *selected += 1;
            }
        }
    }

    /// User confirmed the commit type. Transition to Creating phase.
    pub fn start_popup_confirm(&mut self) {
        if let Some(popup) = &mut self.start_popup {
            if let StartPopupPhase::ChoosingType { selected } = &popup.phase {
                let commit_type = COMMIT_TYPES[*selected].to_string();
                popup.phase = StartPopupPhase::Creating { commit_type };
            }
        }
    }

    /// Spawns the start-ticket work in a background thread. Only runs when in Creating phase.
    pub fn run_start_ticket(&mut self) {
        let popup = match self.start_popup.as_ref() {
            Some(p) => p,
            None => return,
        };
        let commit_type = match &popup.phase {
            StartPopupPhase::Creating { commit_type } => commit_type.clone(),
            _ => return,
        };
        let key = popup.ticket_key.clone();
        let config = self.config.clone();

        let (tx, rx) = mpsc::channel();
        self.start_receiver = Some(rx);
        thread::spawn(move || {
            let result = (|| {
                jira::start_workitem(&key)
                    .map_err(|e| format!("Failed to start ticket: {}", e))?;
                crate::worktree::create_worktree(&key, &commit_type, &config)
                    .map_err(|e| format!("Ticket started but worktree failed: {}", e))
            })();
            let _ = tx.send(result);
        });
    }

    pub fn poll_start_ticket(&mut self) {
        let rx = match &self.start_receiver {
            Some(rx) => rx,
            None => return,
        };
        match rx.try_recv() {
            Ok(result) => {
                if let Some(popup) = self.start_popup.as_mut() {
                    if result.is_ok() {
                        self.detail_cache.remove(&popup.ticket_key);
                    }
                    popup.phase = StartPopupPhase::Done(result);
                }
                self.start_receiver = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(popup) = self.start_popup.as_mut() {
                    popup.phase = StartPopupPhase::Done(Err("Background task disconnected".to_string()));
                }
                self.start_receiver = None;
            }
        }
    }

    pub fn close_start_popup(&mut self) {
        self.start_popup = None;
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
                    self.request_current_detail();
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
                    self.request_current_detail();
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
                self.epics.clear();
                self.selected_epic = None;
                self.loading_epics = false;
                self.epics_receiver = None;
                self.active_pane = Pane::Tickets;
                true
            }
            Pane::Tickets if self.current_ticket().is_some() => {
                self.active_pane = Pane::Detail;
                // If detail already loaded/cached, just switch focus
                if self.detail.is_some() {
                    return false;
                }
                self.loading_detail = true;
                self.request_current_detail();
                false
            }
            _ => false,
        }
    }

    pub fn perform_pending_load(&mut self) {
        if self.loading_tickets {
            self.load_workitems();
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

    // --- Epic popup ---

    pub fn open_epic_popup(&mut self) {
        if self.active_pane != Pane::Tickets || self.projects.is_empty() {
            return;
        }
        // If epics not loaded and not currently loading, start fetch
        if self.epics.is_empty() && !self.loading_epics {
            let project_key = self.projects[self.project_index].key.clone();
            self.start_epic_fetch(&project_key);
        }
        self.show_epic_popup = true;
        // Set popup index to current selection
        self.epic_popup_index = match &self.selected_epic {
            None => 0,
            Some(key) => self
                .epics
                .iter()
                .position(|e| &e.key == key)
                .map(|i| i + 1) // +1 for "All epics"
                .unwrap_or(0),
        };
    }

    pub fn close_epic_popup(&mut self) {
        self.show_epic_popup = false;
    }

    /// Select epic from popup. Returns true if filter changed (needs reload).
    pub fn select_epic(&mut self) -> bool {
        self.show_epic_popup = false;
        let new_epic = if self.epic_popup_index == 0 {
            None
        } else {
            self.epics
                .get(self.epic_popup_index - 1)
                .map(|e| e.key.clone())
        };
        if new_epic != self.selected_epic {
            self.selected_epic = new_epic;
            true
        } else {
            false
        }
    }

    pub fn epic_popup_up(&mut self) {
        if self.epic_popup_index > 0 {
            self.epic_popup_index -= 1;
        }
    }

    pub fn epic_popup_down(&mut self) {
        let max = self.epics.len(); // 0 = "All epics", so max index = epics.len()
        if self.epic_popup_index < max {
            self.epic_popup_index += 1;
        }
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
