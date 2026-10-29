# The Workspace

The Workspace runs your own Claude Code, Codex or a shell in any folder or git repo on your computer, each in a real terminal in a tab, signed in with your own accounts. A session reads the vault but changes it only through Brainstead's tools, so every change it makes is in Changes with Revert.

[Folders](#folders) · [Sessions](#sessions) · [Beside a session](#beside-a-session) · [Your own terminal](#your-own-terminal) · [Projects and folders](#projects-and-folders) · [How the vault is kept safe](#how-the-vault-is-kept-safe)

## Folders

The Workspace (⌘7, Ctrl+7 on Windows) lists folders down the left:

- **Roots**: folders whose git repos are listed, such as `~/repos`. **Add folder…** adds one; with none yet, **Add ~/repos** does it in one click. Each repo shows its branch, an amber dot when it has uncommitted changes, and the project it belongs to. The cross on a root's heading stops listing it; nothing on disk changes.
- **Pinned**: folders you keep at the top (**Pin** on a folder).
- **Recent**: folders sessions started in, newest first.

A folder inside the vault is greyed: the vault changes only through Brainstead, so you use Ask there.

## Sessions

Click a folder to see its sessions, its past Claude Code sessions and **New session**.

- **New session** starts **Claude Code**, **Codex** or **Shell** in the folder, with an optional first message, and for Claude Code an optional git worktree to work in.
- **Past Claude Code sessions** are the folder's earlier sessions, newest first. **Resume** picks one up in the Workspace; **Resume in your terminal** picks it up in your terminal app.

Each session is a tab. A spinner means it's working, a blue dot that it's waiting for you, and a grey dot that it finished while you were elsewhere. Only Claude Code says how it stands; a Codex or shell session shows a green ring while it runs. The sidebar's Workspace count (in bold) is the sessions waiting for you. Switching tabs, or to another screen, leaves the sessions running, and coming back redraws them.

Keys typed in a terminal go to the program, so Brainstead's own single keys (`?`, Esc) don't fire there. ⌘ shortcuts still work on a Mac, and Ctrl+Shift ones on Windows.

Closing a tab stops its session. An ended session says how it ended, and **Restart** starts it again in its place; a Claude Code session carries on where it left off.

## Beside a session

- **Project**: the folder's project and its next actions; click it to open it in Projects.
- **Files changed**: what the session has changed. A file in the vault opens; any other copies its path.
- **Wrap up** (Claude Code): types Brainstead's `/mcp__brainstead__wrap_up` into the session. Claude Code then ticks off the tasks it did, adds new ones, and records what it found and decided, all through Brainstead's tools, and says what it recorded.
- **Open in your terminal**: moves the session to your terminal app, where it carries on.
- **Copy command**: the exact command that starts it in any terminal.
- **Stop** and **Restart**.

The **Ask** button beside `?` starts a chat about the folder showing.

## Your own terminal

**Open in your terminal**, **Resume in your terminal** and **Copy command** start the same guarded launch in your own terminal (Terminal on a Mac, Windows Terminal on Windows), or any terminal you paste the command into. A session there can read the vault but changes it only through Brainstead's tools, the same as in the Workspace. While it runs it shows in the Workspace as a tab with how it stands and the files it changed; you type into it in your terminal.

A session can move between the two with `claude --resume`: **Open in your terminal** does it for you.

## Projects and folders

A project note can list the folders its work happens in, as `folders:` in its properties: paths such as `~/repos/orbit-app`, or bare folder names. On Projects, a project's **Folders** edits the list. A folder belongs to the project whose folders name it or a folder it's in (the deepest wins), else to the project named like the folder, so `orbit-app` belongs to Orbit App. Completed projects count only when nothing else matches.

A session knows its folder's project when it starts, so it records findings, tasks and decisions there. Changes names the folder and project a session's change came from, as `orbit-app · Orbit App`.

## How the vault is kept safe

- A session can't start in the vault, or in a folder inside it.
- Claude Code runs with Brainstead's settings: writing or editing a file in the vault is denied. A shell command that names the vault, or runs in it, may only use programs that read (`cat`, `grep`, `ls`, `head`, `git log` and the like); anything else there is refused, so scripts and copies out of the vault are refused too. A command that doesn't touch the vault runs as usual.
- The check reads the command line, not what a script file does when it runs, and it doesn't cover your own MCP servers' tools. Changes stays the one way into the vault.
- Codex runs in its workspace-write sandbox, which keeps it to the folder it started in.
- Each gets Brainstead's tools, which make every change through the app: listed in Changes with Revert, held when a check fails, exactly as for Ask.
- Shell is you typing, so nothing is guarded.

An assistant can use the Workspace too: list its folders and sessions, add and pin folders, start a Claude Code or Codex session, type into one and stop one. It never starts or types into a session when nobody is watching. It can't start a shell session or type into one, since a shell isn't guarded: you start a shell yourself. Nor can it type into a session running in your own terminal.
