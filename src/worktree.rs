use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::LazyJiraConfig;

/// Create a git worktree for a ticket, copy files, and run setup commands.
/// `commit_type` is a conventional commit key (e.g. "feat", "fix", "refactor").
/// Returns the worktree path on success.
pub fn create_worktree(key: &str, commit_type: &str, config: &LazyJiraConfig) -> Result<String> {
    let key_lower = key.to_lowercase();
    let folder_name = format!("{}{}-{}", config.worktree_folder_prefix, commit_type, key_lower);
    let branch = format!("{}{}/{}", config.worktree_branch_prefix, commit_type, key_lower);
    let base_dir = Path::new(&config.worktree_dir);
    let worktree_path = base_dir.join(&folder_name);

    // Create the worktree with a new branch
    let output = Command::new("git")
        .args(["worktree", "add", "-b", &branch, &worktree_path.to_string_lossy()])
        .output()
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr);
    }

    // Resolve the absolute worktree path
    let abs_worktree = fs::canonicalize(&worktree_path)
        .unwrap_or_else(|_| worktree_path.to_path_buf());

    // Copy files matching worktree_copy globs
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    for pattern in &config.worktree_copy {
        copy_glob_matches(&cwd, pattern, &abs_worktree)?;
    }

    // Run worktree_commands sequentially
    for cmd in &config.worktree_commands {
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&abs_worktree)
            .output()
            .with_context(|| format!("Failed to run command: {}", cmd))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Command '{}' failed: {}", cmd, stderr);
        }
    }

    Ok(abs_worktree.to_string_lossy().into_owned())
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
