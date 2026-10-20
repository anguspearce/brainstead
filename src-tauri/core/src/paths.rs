// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paths the same way on every system: real paths without Windows' `\\?\` prefix, the check that a
//! vault-relative path stays in the vault, a vault-relative path as the file system spells it, the
//! app data folder, and picking a program from `where`'s answer.

use std::io::ErrorKind;
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

/// What separates folders in a vault-relative path on this system: `/`, and `\` on Windows.
/// Elsewhere a backslash is an ordinary character in a name.
pub const SEPARATORS: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };

/// Whether `rel` is a path inside the vault: not empty, not rooted (`/x`, `\x`, `\\?\C:\x`), no
/// `..` and no empty segment, with `/` and `\` both separators. Joining a rooted path onto the
/// vault's root would replace the root, and `..` would climb out of it. On Windows a colon is
/// refused too, and so is a segment that ends in a dot or a space (`.` itself aside): a Windows
/// file name can't have either, so `C:\x` and `C:x` name a drive, `Templates::$INDEX_ALLOCATION`
/// a folder's stream (the folder itself), and `Templates./x.md` or `x.md ` another file's name,
/// as Win32 drops a trailing dot, and a trailing space from the last name. Elsewhere
/// `Q: Ideas.md` and `Notes.` are ordinary names.
pub fn is_vault_relative(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with(['/', '\\'])
        && !(cfg!(windows) && (rel.contains(':') || rel.split(['/', '\\']).any(|s| s != "." && s.ends_with(['.', ' ']))))
        && !rel.split(['/', '\\']).any(|s| s == ".." || s.is_empty())
        && Path::new(rel).components().all(|c| !matches!(c, Component::Prefix(_) | Component::RootDir | Component::ParentDir))
}

/// The vault-relative path `rel` names, spelled as the file system has it: each folder and the
/// file by its own name, with `/` between them. The rules for agent changes go by the path (a
/// change in `Templates/` waits for the user), and one file has many spellings: any case on
/// Windows and macOS, and on Windows `\` between folders and the 8.3 short name (`TEMPLA~1`).
/// The longest part of `rel` that is there is read back from the file system (`canonical`); the
/// rest, for a file that isn't there yet, keeps its spelling below it. Err when `rel` isn't a path
/// in the vault (`is_vault_relative`, or a `.` segment) or leads out of it (a link to a folder
/// elsewhere).
pub fn vault_rel(root: &Path, rel: &str) -> Result<String, String> {
    spelled(root, rel, false)
}

/// `vault_rel` for the name a file is to have (a rename's): its folders as the file system spells
/// them, the name as given. Renaming `idea.md` to `Idea.md` changes only the case, and on Windows
/// and macOS `vault_rel` would read the new name back as the old one, the same file.
pub fn vault_rel_new(root: &Path, rel: &str) -> Result<String, String> {
    spelled(root, rel, true)
}

/// The 8.3 short name Windows gave the last part of `p`, or None when it gave none (the volume
/// doesn't make them, or the name is short already). For tests, this crate's and the MCP server's:
/// one of the spellings a path is read back from.
#[cfg(windows)]
#[doc(hidden)]
pub fn short_name(p: &Path) -> Option<String> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let long: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
    let mut buf = vec![0u16; 4096];
    // SAFETY: `long` ends in a NUL, and the call writes at most `buf.len()` units into `buf`.
    let n = unsafe { windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(long.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    let n = usize::try_from(n).ok().filter(|&n| n > 0 && n < buf.len())?;
    let short = PathBuf::from(std::ffi::OsString::from_wide(&buf[..n]));
    let name = short.file_name()?.to_string_lossy().into_owned();
    (Some(name.as_str()) != p.file_name().and_then(|f| f.to_str())).then_some(name)
}

fn spelled(root: &Path, rel: &str, keep_name: bool) -> Result<String, String> {
    let refuse = || format!("Not a path in the vault: {rel}");
    if !is_vault_relative(rel) || rel.split(['/', '\\']).any(|s| s == ".") {
        return Err(refuse());
    }
    let root = canonical(root).map_err(|e| format!("Couldn't read the vault folder: {e}"))?;
    let parts: Vec<&str> = rel.split(SEPARATORS).collect();
    for there in (0..=parts.len() - usize::from(keep_name)).rev() {
        let real = match canonical(&parts[..there].iter().fold(root.clone(), |p, s| p.join(s))) {
            Ok(real) => real,
            // Not there (or not a name the system takes): the folder above it is tried.
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory | ErrorKind::InvalidFilename) => continue,
            Err(e) => return Err(format!("Couldn't read {rel}: {e}")),
        };
        let inside = real.strip_prefix(&root).map_err(|_| format!("{} (it leads out of the vault)", refuse()))?;
        let mut out: Vec<String> = inside.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        out.extend(parts[there..].iter().map(|s| s.to_string()));
        return if out.is_empty() { Err(refuse()) } else { Ok(out.join("/")) };
    }
    Err(refuse())
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

    #[cfg(windows)]
    #[test]
    fn another_name_for_a_file_is_not_in_the_vault_on_windows() {
        for bad in [
            "Templates./Meeting.md",
            "Templates /Meeting.md",
            "Templates\\Meeting.md.",
            "Templates/Meeting.md ",
            "Templates/.../Meeting.md",
            "Templates::$INDEX_ALLOCATION/Meeting.md",
            "Templates/Meeting.md::$DATA",
        ] {
            assert!(!is_vault_relative(bad), "{bad}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_colon_is_an_ordinary_character_elsewhere() {
        for ok in ["Q: Ideas.md", "wiki/Q: Ideas.md", "C:x", "Notes./x.md", "Notes /x.md"] {
            assert!(is_vault_relative(ok), "{ok}");
        }
    }

    /// A vault with `Templates/Meeting.md`.
    fn templates() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Templates")).unwrap();
        std::fs::write(d.path().join("Templates/Meeting.md"), "x").unwrap();
        d
    }

    #[test]
    fn vault_rel_reads_a_path_back_as_the_file_system_spells_it() {
        let d = templates();
        let root = d.path();
        let ok = |rel: &str| vault_rel(root, rel).unwrap();
        assert_eq!(ok("Templates/Meeting.md"), "Templates/Meeting.md");
        // Not there yet: the folders that are, then the rest as given.
        assert_eq!(ok("Templates/New.md"), "Templates/New.md");
        assert_eq!(ok("Templates/Sub/New.md"), "Templates/Sub/New.md");
        assert_eq!(ok("New.md"), "New.md");
        // A system that doesn't mind case (Windows, macOS by default) names the file in any case.
        if root.join("TEMPLATES/MEETING.MD").is_file() {
            assert_eq!(ok("templates/meeting.md"), "Templates/Meeting.md");
            assert_eq!(ok("TEMPLATES/New.md"), "Templates/New.md");
        }
        for bad in ["", "/x.md", "../x.md", "Templates/../../x.md", "Templates/./Meeting.md", "a//b.md"] {
            assert!(vault_rel(root, bad).is_err(), "{bad}");
        }
        // A rename keeps the name it's given: idea.md to Idea.md is a rename on any system.
        std::fs::write(root.join("idea.md"), "x").unwrap();
        assert_eq!(vault_rel_new(root, "Idea.md").unwrap(), "Idea.md");
        if root.join("TEMPLATES").is_dir() {
            assert_eq!(vault_rel_new(root, "templates/Idea.md").unwrap(), "Templates/Idea.md");
        }
    }

    #[cfg(unix)]
    #[test]
    fn vault_rel_follows_links_and_refuses_one_out_of_the_vault() {
        let d = templates();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("x.md"), "x").unwrap();
        std::os::unix::fs::symlink(d.path().join("Templates"), d.path().join("Snippets")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), d.path().join("Out")).unwrap();
        assert_eq!(vault_rel(d.path(), "Snippets/Meeting.md").unwrap(), "Templates/Meeting.md");
        assert!(vault_rel(d.path(), "Out/x.md").unwrap_err().contains("leads out of the vault"));
    }

    #[cfg(windows)]
    #[test]
    fn vault_rel_reads_back_every_windows_spelling() {
        let d = templates();
        let root = d.path();
        for rel in ["Templates\\Meeting.md", "templates\\MEETING.md", "TEMPLATES/meeting.MD"] {
            assert_eq!(vault_rel(root, rel).unwrap(), "Templates/Meeting.md", "{rel}");
        }
        assert_eq!(vault_rel(root, "templates\\Sub\\New.md").unwrap(), "Templates/Sub/New.md");
        match short_name(&root.join("Templates")) {
            Some(short) => {
                assert_eq!(vault_rel(root, &format!("{short}/Meeting.md")).unwrap(), "Templates/Meeting.md");
                assert_eq!(vault_rel(root, &format!("{short}\\New.md")).unwrap(), "Templates/New.md");
                assert_eq!(vault_rel_new(root, &format!("{short}/Idea.md")).unwrap(), "Templates/Idea.md");
            }
            None => eprintln!("This volume makes no 8.3 names, so the short spelling isn't tried."),
        }
        // Win32 drops a trailing dot, and a trailing space from the last name, so these name the
        // file too, and are refused.
        for bad in ["Templates./Meeting.md", "Templates/Meeting.md.", "Templates/Meeting.md "] {
            assert!(root.join(bad).is_file(), "{bad} names the file");
            assert!(vault_rel(root, bad).is_err(), "{bad}");
        }
        // As are a folder's trailing space, which no file can have, and a colon, which names a
        // stream (NTFS's for a folder is the folder).
        for bad in ["Templates /Meeting.md", "Templates::$INDEX_ALLOCATION/Meeting.md"] {
            assert!(vault_rel(root, bad).is_err(), "{bad}");
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
