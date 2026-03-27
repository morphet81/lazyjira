# lazyjira

A terminal UI for browsing and managing Jira tickets, inspired by lazygit. Built with Rust and [ratatui](https://github.com/ratatui/ratatui).

Lazyjira uses [acli](https://bobswift.atlassian.net/wiki/spaces/ACLI/overview) (Atlassian CLI) under the hood to communicate with Jira.

## Installation

Requires Rust and `acli` configured for your Jira instance.

```sh
./install.sh
```

This builds a release binary and copies it to `~/.cargo/bin/`.

## Releasing

Maintainers bump the semver in `Cargo.toml`, tag, and push; [GitHub Actions](https://github.com/morphet81/lazyjira/actions) then builds binaries and publishes a [release](https://github.com/morphet81/lazyjira/releases) with archives per platform.

1. Install [cargo-edit](https://github.com/killercup/cargo-edit) once: `cargo install cargo-edit`
2. From the repo root: `./scripts/bump-version.sh patch` (or `minor` / `major`)
3. Push the commit and tag (the script prints the exact commands), or run `./scripts/bump-version.sh --push patch` to push immediately

Prebuilt assets use names like `lazyjira-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` (Linux and macOS) or `.zip` on Windows, each with a `.sha256` checksum file.

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
| `Up` / `Down` | Navigate commit type list / agent list |
| `Enter` | Confirm selected type or agent and start |
| `Esc` | Cancel (type selection) / skip agent launch (agent selection) |
| Any key | Dismiss popup (after operation completes, when no agent choice) |

For bug tickets, the type is automatically set to `fix`. For all other ticket types, a popup lets you choose a conventional commit type (`feat`, `fix`, `refactor`, `chore`, etc.).

When multiple AI agents are configured (e.g. `ai_agent = ["claude", "cursor"]`), the success popup shows an agent selector instead of "Press any key to close". Pick an agent with `Enter` or press `Esc` to open the Zellij tab without launching an agent.

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
| `ai_agent` | string or array | `"none"` | AI agent to open in a Zellij pane after creating the worktree. Accepts a single value (`"claude"`, `"cursor"`, `"none"`) or an array (`["claude", "cursor"]`). When multiple agents are given, a chooser popup appears after worktree creation. Requires `zellij_tab = true` and running inside Zellij. |
| `custom_agent_prompt` | string | _(unset)_ | Custom prompt template for the AI agent session. Use `$details` as a placeholder for the ticket content (summary, description, etc.). When unset, defaults to `"Address the following ticket: <ticket details>"`. See [Agent prompt customization](#agent-prompt-customization). |

### Example

```toml
worktree_dir = ".."
worktree_folder_prefix = "myproject-"
worktree_branch_prefix = "myproject-"
worktree_copy = [".env", ".vscode/**"]
worktree_commands = ["npm install"]
conventional_commits_worktree_prefix = true
zellij_tab = true
ai_agent = ["claude", "cursor"]
custom_agent_prompt = "Address the following ticket: $details"
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

### Agent prompt customization

When `ai_agent` is set to `"claude"`, `"cursor"`, or an array like `["claude", "cursor"]`, lazyjira opens an AI agent session in a right Zellij pane after creating the worktree. When multiple agents are configured, a chooser popup lets you pick which one to launch. You can customize the prompt using `custom_agent_prompt`.

The special placeholder `$details` is replaced with the ticket content (summary, description, and any custom text fields, with labels). If `$details` is not included in your prompt, the ticket content is **not** appended automatically — only your exact prompt is used.

**Examples:**

```toml
# Use Claude Code:
ai_agent = "claude"

# Use Cursor CLI agent:
ai_agent = "cursor"

# Offer a choice between both agents after worktree creation:
# ai_agent = ["claude", "cursor"]

# Default behavior (when custom_agent_prompt is not set):
# → "Address the following ticket: Summary: ... Description: ..."

# Include ticket details with a custom instruction:
custom_agent_prompt = "Fix the bug described in this ticket: $details"
# → "Fix the bug described in this ticket: Summary: ... Description: ..."

# Use a prompt without ticket details:
custom_agent_prompt = "Hello, help me with this codebase"
# → "Hello, help me with this codebase"

# Reference details anywhere in the prompt:
custom_agent_prompt = "Given these requirements: $details\n\nPlease implement this feature following existing patterns."
# → "Given these requirements: Summary: ... Description: ...\n\nPlease implement this feature following existing patterns."
```
