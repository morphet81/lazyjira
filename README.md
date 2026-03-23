# lazyjira

A terminal UI for browsing and managing Jira tickets, inspired by lazygit. Built with Rust and [ratatui](https://github.com/ratatui/ratatui).

Lazyjira uses [acli](https://bobswift.atlassian.net/wiki/spaces/ACLI/overview) (Atlassian CLI) under the hood to communicate with Jira.

## Installation

Requires Rust and `acli` configured for your Jira instance.

```sh
./install.sh
```

This builds a release binary and copies it to `~/.cargo/bin/`.

## Usage

```sh
lazyjira
```

The interface has three panes: **Projects** (1), **Tickets** (2), and **Detail** (3).

## Keyboard shortcuts

### Global

| Key | Description |
|-----|-------------|
| `q` | Quit |
| `Tab` | Cycle focus between panes |
| `1` / `2` / `3` | Jump to pane by number |
| `Up` / `Down` | Navigate items in the focused pane |
| `Left` / `Right` | Navigate columns (Tickets) or panes |
| `Enter` | Select project / open ticket detail |
| `Shift+C` | Create `.lazyjira` config file with example settings |

### Tickets pane

| Key | Description |
|-----|-------------|
| `Left` / `Right` | Switch board columns |
| `Shift+Up` | Sort tickets by key ascending |
| `Shift+Down` | Sort tickets by key descending |
| `P` | Sort tickets by priority |
| `e` | Open epic filter popup |
| `r` | Refresh tickets |
| `s` | Start ticket — assign to you, transition to In Progress, and create a git worktree |

### Epic filter popup

| Key | Description |
|-----|-------------|
| `Up` / `Down` | Navigate epics |
| `Enter` | Select epic filter |
| `Esc` | Close popup |

### Detail pane

| Key | Description |
|-----|-------------|
| `Up` / `Down` | Select editable field |
| `e` | Edit selected field (enters vi normal mode) |

### Vi normal mode (editing a field)

| Key | Description |
|-----|-------------|
| `h` / `l` | Move cursor left / right |
| `j` / `k` | Move cursor down / up |
| `0` | Move to start of line |
| `$` | Move to end of line |
| `w` | Move forward one word |
| `b` | Move back one word |
| `i` | Enter insert mode at cursor |
| `a` | Enter insert mode after cursor |
| `I` | Enter insert mode at start of line |
| `A` | Enter insert mode at end of line |
| `o` | Open new line below |
| `O` | Open new line above |
| `x` | Delete character under cursor |
| `dd` | Delete entire line |
| `D` | Delete to end of line |
| `G` | Jump to last line |
| `gg` | Jump to first line |
| `Esc` | Save changes and return to viewing mode |

### Vi insert mode

| Key | Description |
|-----|-------------|
| Any character | Insert text |
| `Backspace` | Delete character before cursor |
| `Enter` | Insert newline |
| Arrow keys | Move cursor |
| `Esc` | Return to vi normal mode |

### Start-ticket popup

| Key | Description |
|-----|-------------|
| `Up` / `Down` | Navigate commit type list |
| `Enter` | Confirm selected type and start |
| `Esc` | Cancel |
| Any key | Dismiss popup (after operation completes) |

For bug tickets, the type is automatically set to `fix`. For all other ticket types, a popup lets you choose a conventional commit type (`feat`, `fix`, `refactor`, `chore`, etc.).

## Configuration

Lazyjira reads an optional `.lazyjira` file (TOML) from the working directory. Press `Shift+C` to generate one with example settings.

### Options

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `worktree_dir` | string | `"."` | Directory where git worktrees are created. |
| `worktree_folder_prefix` | string | `""` | Prefix prepended to worktree folder names. E.g. with `"myproject-"`, folder becomes `myproject-feat-proj-123`. |
| `worktree_branch_prefix` | string | `""` | Prefix prepended to worktree branch names. E.g. with `"myproject-"`, branch becomes `myproject-feat/proj-123`. |
| `worktree_copy` | string array | `[]` | Files or glob patterns to copy from the project root into new worktrees (e.g. `[".env", ".vscode/**"]`). |
| `worktree_commands` | string array | `[]` | Shell commands to run inside the new worktree directory after creation, in order (e.g. `["npm install"]`). |
| `conventional_commits_worktree_prefix` | bool | `false` | Prompt for a conventional commit type (`feat`, `fix`, `refactor`, ...) when starting a ticket. When `false`, uses `feat` for non-bugs and `fix` for bugs. |
| `zellij_tab` | bool | `true` | Automatically open a Zellij tab for the new worktree when running inside Zellij. |

### Example

```toml
worktree_dir = ".."
worktree_folder_prefix = "myproject-"
worktree_branch_prefix = "myproject-"
worktree_copy = [".env", ".vscode/**"]
worktree_commands = ["npm install"]
conventional_commits_worktree_prefix = true
```

When you press `s` on a "To Do" ticket (e.g. `NERO-1234`), lazyjira will:

1. If `conventional_commits_worktree_prefix` is enabled, prompt you to choose a type (`feat`, `fix`, `refactor`, etc.) — bugs always default to `fix`
2. Assign the ticket to you and transition it to **In Progress**
3. Run `git fetch` to ensure branches are up to date
4. Create a git worktree with folder `<worktree_folder_prefix><type>-nero-1234` and branch `<worktree_branch_prefix><type>/nero-1234`
5. Copy files matching `worktree_copy` patterns into the worktree
6. Run each `worktree_commands` entry inside the new worktree
7. If running inside **Zellij**, open a new tab named after the ticket key with the worktree as working directory

A progress popup shows each step as it executes, including the name of any custom commands being run.
