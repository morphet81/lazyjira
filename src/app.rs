use crate::jira::{self, JiraProject, WorkItem, WorkItemDetail};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Projects,
    Tickets,
    Detail,
}

/// A board column derived from status categories.
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub items: Vec<WorkItem>,
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

    fn build_columns(&mut self, items: Vec<WorkItem>) {
        // Group by statusCategory.id, sorted by category order: To Do(2) → In Progress(4) → Done(3)
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

        // Custom sort: category 2 (To Do) first, then 4 (In Progress), then 3 (Done), then others
        let order = |id: &u32| -> u32 {
            match id {
                2 => 0, // To Do
                4 => 1, // In Progress
                3 => 2, // Done
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
                    self.detail = Some(detail);
                }
                Err(e) => {
                    self.status_message = format!("Error: {}", e);
                }
            }
        }
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

    // Input handling

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
            Pane::Detail => {}
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
            Pane::Detail => {}
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
        }
    }

    pub fn move_right(&mut self) {
        if self.active_pane == Pane::Tickets && !self.columns.is_empty() {
            self.column_index = (self.column_index + 1) % self.columns.len();
            self.ticket_index = 0;
            self.detail = None;
        }
    }

    pub fn enter(&mut self) {
        match self.active_pane {
            Pane::Projects => {
                self.load_workitems();
                self.active_pane = Pane::Tickets;
            }
            Pane::Tickets => {
                self.load_detail();
            }
            Pane::Detail => {}
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
}
