use serde::Deserialize;
use std::fs;
use std::path::Path;

const CONFIG_FILE: &str = ".lazyjira";

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct LazyJiraConfig {
    /// Directory where worktrees are created. Defaults to current folder (".").
    #[serde(default = "default_worktree_dir")]
    pub worktree_dir: String,

    /// Prefix prepended to worktree folder names (e.g. "myproject-").
    #[serde(default)]
    pub worktree_folder_prefix: String,

    /// Prefix prepended to worktree branch names (e.g. "myproject-").
    #[serde(default)]
    pub worktree_branch_prefix: String,

    /// Files or glob patterns to copy from the project root to a new worktree.
    #[serde(default)]
    pub worktree_copy: Vec<String>,

    /// Commands to run in the new worktree directory after creation (in order).
    #[serde(default)]
    pub worktree_commands: Vec<String>,

    /// Prompt for a conventional commit type when starting a ticket (default: false).
    #[serde(default)]
    pub conventional_commits: bool,
}

fn default_worktree_dir() -> String {
    ".".to_string()
}

const EXAMPLE_CONFIG: &str = r#"# lazyjira configuration

# Directory where worktrees are created (default: current folder).
# worktree_dir = ".."

# Prefix prepended to worktree folder names (default: empty).
# Example: with prefix "myproject-", folder becomes "myproject-feat-proj-123".
# worktree_folder_prefix = ""

# Prefix prepended to worktree branch names (default: empty).
# Example: with prefix "myproject-", branch becomes "myproject-feat/proj-123".
# worktree_branch_prefix = ""

# Files or glob patterns to copy from the project root into new worktrees.
# Example: copy environment and IDE config files.
# worktree_copy = [".env", ".vscode/**"]

# Commands to run inside the new worktree directory after creation (in order).
# Example: install dependencies.
# worktree_commands = ["npm install"]

# Prompt for a conventional commit type (feat, fix, refactor, ...) when
# starting a ticket. When false, uses "feat" for all non-bug tickets and
# "fix" for bugs (default: false).
# conventional_commits = false
"#;

impl LazyJiraConfig {
    /// Create a `.lazyjira` config file with example content.
    /// Returns Ok(true) if created, Ok(false) if it already exists.
    pub fn create_default() -> Result<bool, std::io::Error> {
        let path = Path::new(CONFIG_FILE);
        if path.exists() {
            return Ok(false);
        }
        fs::write(path, EXAMPLE_CONFIG)?;
        Ok(true)
    }

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
