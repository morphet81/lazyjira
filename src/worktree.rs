use anyhow::{Context, Result};
use log::{debug, info};
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

/// Check whether we are running inside a Zellij session.
/// `zellij action` commands only work when the ZELLIJ env var is set,
/// meaning we are inside an active Zellij session.
pub fn is_inside_zellij() -> bool {
    let inside = std::env::var("ZELLIJ").is_ok();
    info!("is_inside_zellij() = {} (ZELLIJ env var)", inside);
    inside
}

/// Open a new Zellij tab named after the ticket, wait for it to be ready,
/// then navigate to the worktree directory.
pub fn open_zellij_tab(name: &str, cwd: &str) {
    info!("open_zellij_tab: name={:?}, cwd={:?}", name, cwd);

    // Create a new tab named after the ticket key.
    let new_tab_result = Command::new("zellij")
        .args(["action", "new-tab", "--name", name])
        .output();
    let ok = new_tab_result
        .as_ref()
        .map(|o| o.status.success())
        .unwrap_or(false);
    debug!("  new-tab result: ok={}, output={:?}", ok, new_tab_result);

    if !ok {
        info!("  new-tab failed, aborting");
        return;
    }

    // Wait for the new tab's shell to initialise.
    debug!("  sleeping 500ms for shell init");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Navigate to the worktree directory in the new tab.
    let cd_cmd = format!("cd '{}'\n", cwd.replace('\'', "'\\''"));
    let write_result = Command::new("zellij")
        .args(["action", "write-chars", &cd_cmd])
        .output();
    debug!("  write-chars result: {:?}", write_result);
    info!("open_zellij_tab: done for {}", name);
}

/// Open a right pane in the current Zellij tab, navigate to the worktree,
/// and launch a Claude session with the ticket content as the initial prompt.
pub fn open_zellij_claude_pane(cwd: &str, ticket_text: &str) {
    info!("open_zellij_claude_pane: cwd={:?}", cwd);

    // Open a pane to the right of the current one.
    let ok = Command::new("zellij")
        .args(["action", "new-pane", "--direction", "right"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !ok {
        info!("  new-pane failed, aborting");
        return;
    }

    // Wait for the shell to initialise.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Navigate to the worktree directory.
    let cd_cmd = format!("cd '{}'\n", cwd.replace('\'', "'\\''"));
    let _ = Command::new("zellij")
        .args(["action", "write-chars", &cd_cmd])
        .output();

    std::thread::sleep(std::time::Duration::from_millis(300));

    // Launch claude with the prompt.
    let escaped = ticket_text.replace('\'', "'\\''");
    let claude_cmd = format!("claude '{}'\n", escaped);
    let _ = Command::new("zellij")
        .args(["action", "write-chars", &claude_cmd])
        .output();

    info!("open_zellij_claude_pane: done");
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
