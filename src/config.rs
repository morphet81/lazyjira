use serde::Deserialize;
use std::fs;
use std::path::Path;

const CONFIG_FILE: &str = ".lazyjira";

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct LazyJiraConfig {
    /// Files or glob patterns to copy from the project root to a new worktree.
    #[serde(default)]
    pub worktree_copy: Vec<String>,

    /// Commands to run in the new worktree directory after creation (in order).
    #[serde(default)]
    pub worktree_commands: Vec<String>,
}

impl LazyJiraConfig {
    pub fn load() -> Self {
        let path = Path::new(CONFIG_FILE);
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Warning: failed to parse {}: {}", CONFIG_FILE, e);
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read {}: {}", CONFIG_FILE, e);
                Self::default()
            }
        }
    }
}
