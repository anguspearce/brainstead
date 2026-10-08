// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paths the same way on every system: real paths without Windows' `\\?\` prefix, the check that a
//! vault-relative path stays in the vault, the app data folder, and picking a program from
//! `where`'s answer.

use std::path::{Component, Path, PathBuf};

/// The real path (links followed, `..` resolved), or an error when it isn't there. On Windows
/// std's `canonicalize` returns a verbatim `\\?\C:\…` path, under which `/` is not a separator,
/// so `root.join("wiki/x.md")` names no file; `dunce` returns the plain `C:\…` form whenever
/// it means the same file.
pub fn canonical(p: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(p)
}

/// `canonical`, or the path as given when it can't be resolved (it isn't there yet, or can't be read).
pub fn real_path(p: &Path) -> PathBuf {
    canonical(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Whether `s` starts with a drive letter and a colon (`C:`, `c:\x`, `C:x`). Rust parses drive
/// prefixes only on Windows, so this is checked by hand for a path that came from Windows.
pub fn has_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Whether `raw` names a place on its own rather than relative to a folder: a root (`/x`, `\x`,
/// `\\server\share`, `\\?\C:\x`) or a drive (`C:\x`, `C:x`). Only `/` roots a path outside
/// Windows, where a backslash and a colon are ordinary characters in a name, so `\Docs` and
/// `C: Notes/x.md` are relative there.
pub fn is_rooted(raw: &str) -> bool {
    raw.starts_with('/') || (cfg!(windows) && (raw.starts_with('\\') || has_drive(raw))) || Path::new(raw).has_root()
}

/// Whether `rel` is a path inside the vault: not empty, not rooted (`/x`, `\x`, `\\?\C:\x`), no
/// `..` and no empty segment, with `/` and `\` both separators. Joining a rooted path onto the
/// vault's root would replace the root, and `..` would climb out of it. A drive (`C:\x`, `C:x`)
/// is refused on Windows only: a colon can't be in a Windows file name, while elsewhere `Q: Ideas.md`
/// is an ordinary note.
pub fn is_vault_relative(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with(['/', '\\'])
        && !(cfg!(windows) && has_drive(rel))
        && !rel.split(['/', '\\']).any(|s| s == ".." || s.is_empty())
        && Path::new(rel).components().all(|c| !matches!(c, Component::Prefix(_) | Component::RootDir | Component::ParentDir))
}

/// The app data folder (settings.json, index.db): `BRAINSTEAD_DATA` when set, otherwise
/// `~/Library/Application Support/Brainstead` on macOS, `%APPDATA%\Brainstead` on Windows and the
/// XDG data folder (`~/.local/share/Brainstead`) on Linux. The app and the standalone MCP server
/// both use it, so they find the same index.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("BRAINSTEAD_DATA").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    default_data_dir()
}

/// `data_dir` without the `BRAINSTEAD_DATA` override.
pub fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")).join("Library/Application Support/Brainstead");
    #[cfg(not(target_os = "macos"))]
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Brainstead")
}

/// The program to run from `where`'s answer (one path per line): the first `.exe`, else the first
/// `.cmd` or `.bat`, among the files that are there. Never an extensionless file: npm installs a
/// shell script beside each `.cmd` shim, and starting it fails with os error 193.
pub fn pick_program(listing: &str) -> Option<PathBuf> {
    let found: Vec<PathBuf> = listing.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.is_file()).collect();
    let ext = |p: &PathBuf, want: &[&str]| p.extension().is_some_and(|e| want.iter().any(|w| e.eq_ignore_ascii_case(w)));
    found.iter().find(|p| ext(p, &["exe"])).or_else(|| found.iter().find(|p| ext(p, &["cmd", "bat"]))).cloned()
}

/// What an npm `.cmd` shim starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShimTarget {
    /// A program the shim runs itself: `"%dp0%\node_modules\…\claude.exe" %*`.
    Exe(PathBuf),
    /// A script the shim runs with node: `"%_prog%" "%dp0%\node_modules\…\index.js" %*`, or the
    /// older `"%~dp0\node.exe" "%~dp0\…\cli.js" %*`.
    Script(PathBuf),
}

/// The target of an npm shim's text, `dir` being the shim's folder (what `%dp0%` and `%~dp0` stand
/// for): the first quoted path under that folder that is a `.js` script, or a program other than
/// node itself. None for anything else, which is then run through cmd.exe as it is.
pub fn parse_npm_shim(text: &str, dir: &Path) -> Option<ShimTarget> {
    text.split('"').skip(1).step_by(2).find_map(|quoted| {
        let rest = quoted.strip_prefix("%dp0%").or_else(|| quoted.strip_prefix("%~dp0"))?;
        let path = rest.split(['\\', '/']).filter(|s| !s.is_empty()).fold(dir.to_path_buf(), |p, s| p.join(s));
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if [".js", ".cjs", ".mjs"].iter().any(|e| name.ends_with(e)) {
            Some(ShimTarget::Script(path))
        } else if name.ends_with(".exe") && name != "node.exe" {
            Some(ShimTarget::Exe(path))
        } else {
            None
        }
    })
}

/// The program and leading arguments that start what the npm shim at `shim` starts, without
/// cmd.exe, which refuses arguments with line breaks (std reports "batch file arguments are
/// invalid") and caps the command line at 8,191 characters. A script runs with the node beside the
/// shim when there is one, else with `node()` (node on PATH). None when `shim` isn't a `.cmd` or
/// `.bat`, can't be read or parsed, or what it names isn't there.
pub fn resolve_npm_shim(shim: &Path, node: impl FnOnce() -> Option<PathBuf>) -> Option<(PathBuf, Vec<String>)> {
    if !shim.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")) {
        return None;
    }
    let dir = shim.parent()?;
    match parse_npm_shim(&std::fs::read_to_string(shim).ok()?, dir)? {
        ShimTarget::Exe(exe) => exe.is_file().then_some((exe, Vec::new())),
        ShimTarget::Script(script) if script.is_file() => {
            let beside = dir.join("node.exe");
            let node = if beside.is_file() { beside } else { node()? };
            Some((node, vec![script.to_string_lossy().into_owned()]))
        }
        ShimTarget::Script(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_relative_paths() {
        for ok in ["wiki/x.md", "a.md", "wiki\\entities\\Orbit App.md", "Me. To Do List.md", "a/./b.md"] {
            assert!(is_vault_relative(ok), "{ok}");
        }
        for bad in ["", "\\\\?\\C:\\x", "\\\\server\\share\\x", "/x", "\\x", "a/../b", "..", "a\\..\\b", "a//b", "a/"] {
            assert!(!is_vault_relative(bad), "{bad}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_is_not_in_the_vault_on_windows() {
        for bad in ["C:\\x", "C:x", "c:/x", "Q: Ideas.md"] {
            assert!(!is_vault_relative(bad), "{bad}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_colon_is_an_ordinary_character_elsewhere() {
        for ok in ["Q: Ideas.md", "wiki/Q: Ideas.md", "C:x"] {
            assert!(is_vault_relative(ok), "{ok}");
        }
    }

    #[test]
    fn rooted_paths() {
        assert!(is_rooted("/x"));
        for r in ["x", "wiki/x.md", "~/x", "1:x"] {
            assert!(!is_rooted(r), "{r}");
        }
        for r in ["\\x", "\\\\?\\C:\\x", "\\\\server\\share\\x", "C:\\x", "C:x", "C: Notes/x.md"] {
            assert_eq!(is_rooted(r), cfg!(windows), "{r}");
        }
    }

    #[test]
    fn real_path_has_no_verbatim_prefix_and_joins_nested_paths() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("wiki")).unwrap();
        std::fs::write(d.path().join("wiki/x.md"), "x").unwrap();
        let root = real_path(d.path());
        assert!(!root.to_string_lossy().starts_with(r"\\?\"), "{}", root.display());
        assert!(root.join("wiki/x.md").is_file());
        let missing = d.path().join("not-there");
        assert_eq!(real_path(&missing), missing);
        assert!(canonical(&missing).is_err());
    }

    #[test]
    fn picks_the_exe_then_the_cmd_never_the_script() {
        let d = tempfile::tempdir().unwrap();
        let (script, cmd, exe) = (d.path().join("claude"), d.path().join("claude.cmd"), d.path().join("claude.exe"));
        for f in [&script, &cmd] {
            std::fs::write(f, "x").unwrap();
        }
        let listing = |ps: &[&PathBuf]| ps.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\r\n");
        assert_eq!(pick_program(&listing(&[&script, &cmd])), Some(cmd.clone()));
        assert_eq!(pick_program(&listing(&[&script])), None);
        std::fs::write(&exe, "x").unwrap();
        assert_eq!(pick_program(&listing(&[&script, &cmd, &exe])), Some(exe));
        assert_eq!(pick_program(""), None);
    }

    // npm's shims as cmd-shim writes them: a native program (Claude Code's npm package), a node
    // script with the `%_prog%` lookup, and the older form with `%~dp0`.
    const SHIM_EXE: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n";
    const SHIM_PROG: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";
    const SHIM_OLD: &str = "@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \"%~dp0\\node_modules\\@github\\copilot\\index.js\" %*\r\n) ELSE (\r\n  @SETLOCAL\r\n  @SET PATHEXT=%PATHEXT:;.JS;=;%\r\n  node  \"%~dp0\\node_modules\\@github\\copilot\\index.js\" %*\r\n)\r\n";

    #[test]
    fn parses_each_npm_shim_form() {
        let dir = Path::new("npm");
        let under = |parts: &[&str]| parts.iter().fold(dir.to_path_buf(), |p, s| p.join(s));
        assert_eq!(
            parse_npm_shim(SHIM_EXE, dir),
            Some(ShimTarget::Exe(under(&["node_modules", "@anthropic-ai", "claude-code", "bin", "claude.exe"])))
        );
        assert_eq!(
            parse_npm_shim(SHIM_PROG, dir),
            Some(ShimTarget::Script(under(&["node_modules", "@openai", "codex", "bin", "codex.js"])))
        );
        assert_eq!(parse_npm_shim(SHIM_OLD, dir), Some(ShimTarget::Script(under(&["node_modules", "@github", "copilot", "index.js"]))));
        assert_eq!(parse_npm_shim("@echo off\r\nstart.exe %*\r\n", dir), None);
    }

    #[test]
    fn resolves_a_shim_to_node_and_its_script() {
        let d = tempfile::tempdir().unwrap();
        let script = d.path().join("node_modules").join("@openai").join("codex").join("bin").join("codex.js");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "").unwrap();
        let shim = d.path().join("codex.cmd");
        std::fs::write(&shim, SHIM_PROG).unwrap();
        let path_node = PathBuf::from("node-on-path.exe");
        let (exe, args) = resolve_npm_shim(&shim, || Some(path_node.clone())).unwrap();
        assert_eq!((exe, args), (path_node, vec![script.to_string_lossy().into_owned()]));
        // The node beside the shim comes first.
        std::fs::write(d.path().join("node.exe"), "").unwrap();
        assert_eq!(resolve_npm_shim(&shim, || None).unwrap().0, d.path().join("node.exe"));
        // A script that isn't there, or a shim that can't be parsed, isn't resolved.
        std::fs::remove_file(&script).unwrap();
        assert_eq!(resolve_npm_shim(&shim, || None), None);
        std::fs::write(&shim, "@echo off\r\n").unwrap();
        assert_eq!(resolve_npm_shim(&shim, || None), None);
    }

    #[test]
    fn data_dir_by_system() {
        let d = default_data_dir();
        assert!(d.ends_with("Brainstead"));
        #[cfg(target_os = "macos")]
        assert!(d.ends_with("Library/Application Support/Brainstead"));
        #[cfg(windows)]
        assert_eq!(Some(d.as_path()), dirs::data_dir().map(|a| a.join("Brainstead")).as_deref());
    }
}
