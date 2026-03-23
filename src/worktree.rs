use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;

use crate::config::LazyJiraConfig;

/// Progress messages sent from the worktree creation thread.
pub enum WorktreeProgress {
    Step(String),
    Done(Result<String, String>),
}

/// Create a git worktree for a ticket, copy files, and run setup commands.
/// Sends progress updates through `progress_tx`. Final message is always `Done`.
pub fn create_worktree(
    key: &str,
    commit_type: &str,
    config: &LazyJiraConfig,
    progress_tx: &mpsc::Sender<WorktreeProgress>,
) {
    let result = create_worktree_inner(key, commit_type, config, progress_tx);
    let _ = progress_tx.send(WorktreeProgress::Done(result));
}

fn create_worktree_inner(
    key: &str,
    commit_type: &str,
    config: &LazyJiraConfig,
    progress_tx: &mpsc::Sender<WorktreeProgress>,
) -> Result<String, String> {
    let key_lower = key.to_lowercase();
    let folder_name = format!("{}{}-{}", config.worktree_folder_prefix, commit_type, key_lower);
    let branch = format!("{}{}/{}", config.worktree_branch_prefix, commit_type, key_lower);
    let base_dir = Path::new(&config.worktree_dir);
    let worktree_path = base_dir.join(&folder_name);

    // Step 1: Fetch
    let _ = progress_tx.send(WorktreeProgress::Step("Fetching branches...".into()));
    run_git(&["fetch"]).map_err(|e| format!("git fetch failed: {}", e))?;

    // Step 2: Create worktree
    let _ = progress_tx.send(WorktreeProgress::Step("Creating worktree...".into()));
    run_git(&["worktree", "add", "-b", &branch, &worktree_path.to_string_lossy()])
        .map_err(|e| format!("git worktree add failed: {}", e))?;

    // Resolve the absolute worktree path
    let abs_worktree = fs::canonicalize(&worktree_path)
        .unwrap_or_else(|_| worktree_path.to_path_buf());

    // Step 3: Copy files
    if !config.worktree_copy.is_empty() {
        let _ = progress_tx.send(WorktreeProgress::Step("Copying files...".into()));
        let cwd = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        for pattern in &config.worktree_copy {
            copy_glob_matches(&cwd, pattern, &abs_worktree)
                .map_err(|e| format!("Copy failed: {}", e))?;
        }
    }

    // Step 4: Run custom commands
    for cmd in &config.worktree_commands {
        let _ = progress_tx.send(WorktreeProgress::Step(format!("Running: {}", cmd)));
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&abs_worktree)
            .output()
            .map_err(|e| format!("Failed to run '{}': {}", cmd, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Command '{}' failed: {}", cmd, stderr));
        }
    }

    Ok(abs_worktree.to_string_lossy().into_owned())
}

fn run_git(args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

/// Open a new Zellij tab with the given name and working directory.
/// Uses a tab layout file so the pane cwd is set correctly.
pub fn open_zellij_tab(name: &str, cwd: &str) {
    let escaped_cwd = cwd.replace('\\', "\\\\").replace('"', "\\\"");
    let layout = format!("pane cwd=\"{}\"", escaped_cwd);
    if let Ok(tmp) = tempfile::Builder::new().suffix(".kdl").tempfile() {
        let path = tmp.path().to_path_buf();
        if std::fs::write(&path, &layout).is_ok() {
            let _ = Command::new("zellij")
                .args([
                    "action", "new-tab",
                    "--layout", &path.to_string_lossy(),
                    "--name", name,
                ])
                .output();
        }
    }
}

fn copy_glob_matches(cwd: &Path, pattern: &str, dest: &Path) -> Result<()> {
    let full_pattern = cwd.join(pattern).to_string_lossy().into_owned();
    let entries = glob::glob(&full_pattern)
        .with_context(|| format!("Invalid glob pattern: {}", pattern))?;

    for entry in entries.flatten() {
        // Get relative path from cwd
        if let Ok(rel) = entry.strip_prefix(cwd) {
            let dest_path = dest.join(rel);
            // Create parent directories if needed
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            if entry.is_file() {
                fs::copy(&entry, &dest_path).with_context(|| {
                    format!("Failed to copy {} to {}", entry.display(), dest_path.display())
                })?;
            }
        }
    }
    Ok(())
}
