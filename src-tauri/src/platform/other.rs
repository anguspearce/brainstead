// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows and Linux: enough to build and run. Windows also registers the capture host and Open at
//! login in the registry (HKCU, no administrator rights needed) and copies rich text.

use std::path::Path;
#[cfg(not(windows))]
use std::path::PathBuf;

#[cfg(not(windows))]
pub fn native_host_dirs() -> Vec<(&'static str, PathBuf)> {
    let Some(base) = dirs::config_dir() else { return vec![] };
    [("Chrome", "google-chrome"), ("Chromium", "chromium"), ("Edge", "microsoft-edge"), ("Brave", "BraveSoftware/Brave-Browser")]
        .into_iter()
        .filter(|(_, d)| base.join(d).is_dir())
        .map(|(n, d)| (n, base.join(d).join("NativeMessagingHosts")))
        .collect()
}

/// The Chromium browsers by name and vendor path: the path is both their key under
/// `HKCU\Software` and their folder under `%LOCALAPPDATA%`.
#[cfg(windows)]
const BROWSERS: &[(&str, &str)] =
    &[("Chrome", r"Google\Chrome"), ("Edge", r"Microsoft\Edge"), ("Brave", r"BraveSoftware\Brave-Browser"), ("Chromium", "Chromium")];

/// Chromium on Windows finds a native messaging host through the default value of
/// `HKCU\Software\<vendor>\NativeMessagingHosts\<name>`, the path of its manifest. One manifest in
/// the app data folder serves every browser.
#[cfg(windows)]
pub fn register_native_host(name: &str, manifest: &serde_json::Value, write: bool) -> Vec<super::NativeHost> {
    let file = super::data_dir().join("nativehost").join(format!("{name}.json"));
    register_in("Software", dirs::data_local_dir().as_deref(), &file, name, manifest, write)
}

/// `register_native_host` under `HKCU\<base>`, with browsers installed when their key is there
/// or their folder is in `local`.
#[cfg(windows)]
fn register_in(
    base: &str,
    local: Option<&Path>,
    file: &Path,
    name: &str,
    manifest: &serde_json::Value,
    write: bool,
) -> Vec<super::NativeHost> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let installed: Vec<_> = BROWSERS
        .iter()
        .filter(|(_, vendor)| hkcu.open_subkey(format!(r"{base}\{vendor}")).is_ok() || local.is_some_and(|l| l.join(vendor).is_dir()))
        .collect();
    if installed.is_empty() {
        return vec![];
    }
    let (_, file_current) = super::manifest_file(file, manifest, write);
    let path = file.to_string_lossy().into_owned();
    installed
        .into_iter()
        .map(|(browser, vendor)| {
            let key = format!(r"{base}\{vendor}\NativeMessagingHosts\{name}");
            let have: Option<String> = hkcu.open_subkey(&key).and_then(|k| k.get_value("")).ok();
            let mut current = file_current && have.as_deref() == Some(path.as_str());
            if !current && write && file_current {
                current = hkcu.create_subkey(&key).and_then(|(k, _)| k.set_value("", &path)).is_ok();
            }
            super::NativeHost { browser, registered: current || have.is_some(), current }
        })
        .collect()
}

pub fn quick_look(path: &Path) -> Result<(), String> {
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// The `Run` key's value: what Windows starts at sign-in.
#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_VALUE: &str = "Brainstead";

/// This program as the `Run` value names it, quoted for a path with spaces. None in a debug build,
/// which would register target\debug at sign-in (as macOS can't register outside an app bundle).
#[cfg(windows)]
fn run_command() -> Option<String> {
    if cfg!(debug_assertions) {
        return None;
    }
    std::env::current_exe().ok().map(|p| format!("\"{}\"", p.display()))
}

/// Whether Windows starts this program at sign-in: the `Run` value names it.
pub fn open_at_login() -> Option<bool> {
    #[cfg(windows)]
    {
        let want = run_command()?;
        Some(run_value(RUN_KEY).is_some_and(|v| v.eq_ignore_ascii_case(&want)))
    }
    #[cfg(not(windows))]
    None
}

pub fn set_open_at_login(on: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let cmd = run_command().ok_or("Opening at login is only set from an installed Brainstead.")?;
        set_run_value(RUN_KEY, on.then_some(cmd.as_str()))
    }
    #[cfg(not(windows))]
    {
        let _ = on;
        Err("Opening at login isn't built for this system yet.".into())
    }
}

#[cfg(windows)]
fn run_value(key: &str) -> Option<String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    hkcu.open_subkey(key).and_then(|k| k.get_value(RUN_VALUE)).ok()
}

/// Sets the `Run` value to `cmd`, or removes it for None (already gone counts as done).
#[cfg(windows)]
fn set_run_value(key: &str, cmd: Option<&str>) -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (k, _) = hkcu.create_subkey(key).map_err(|e| e.to_string())?;
    match cmd {
        Some(c) => k.set_value(RUN_VALUE, &c.to_string()).map_err(|e| e.to_string()),
        None => match k.delete_value(RUN_VALUE) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
    }
}

pub fn reveal(path: &Path) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

pub fn full_disk_access() -> Option<bool> {
    Some(true)
}

pub fn open_full_disk_access_settings() -> Result<(), String> {
    Ok(())
}

/// HTML with its plain text, as the clipboard's "HTML Format" and text together (Windows). Linux
/// has plain text only for now.
pub fn copy_html(html: &str, text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        cb.set().html(html, Some(text)).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (html, text);
        Err("Rich copy isn't available on this system yet.".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    /// A key of the test's own under HKCU\Software, removed when dropped.
    struct TestKey(String);

    impl Drop for TestKey {
        fn drop(&mut self) {
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
        }
    }

    fn test_key(what: &str) -> TestKey {
        TestKey(format!(r"Software\BrainsteadTest-{what}-{}", std::process::id()))
    }

    #[test]
    fn registers_the_host_with_installed_browsers_only() {
        let base = test_key("host");
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        // Edge by its key, Brave by its folder; Chrome and Chromium aren't there.
        hkcu.create_subkey(format!(r"{}\Microsoft\Edge", base.0)).unwrap();
        let local = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(local.path().join(r"BraveSoftware\Brave-Browser")).unwrap();
        let data = tempfile::tempdir().unwrap();
        let file = data.path().join("nativehost").join("com.example.host.json");
        let manifest = serde_json::json!({"name": "com.example.host", "path": r"C:\Apps\Orbit App.exe"});

        // Only reporting: nothing written.
        let seen = register_in(&base.0, Some(local.path()), &file, "com.example.host", &manifest, false);
        assert_eq!(
            seen.iter().map(|b| (b.browser, b.registered, b.current)).collect::<Vec<_>>(),
            [("Edge", false, false), ("Brave", false, false)]
        );
        assert!(!file.exists());

        let done = register_in(&base.0, Some(local.path()), &file, "com.example.host", &manifest, true);
        assert!(done.iter().all(|b| b.registered && b.current), "{done:?}");
        let path: String =
            hkcu.open_subkey(format!(r"{}\Microsoft\Edge\NativeMessagingHosts\com.example.host", base.0)).unwrap().get_value("").unwrap();
        assert_eq!(path, file.to_string_lossy());
        let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(written, manifest);

        // Another build's manifest: registered, not current, until written again.
        let other = serde_json::json!({"name": "com.example.host", "path": r"C:\Other\Orbit App.exe"});
        let seen = register_in(&base.0, Some(local.path()), &file, "com.example.host", &other, false);
        assert!(seen.iter().all(|b| b.registered && !b.current), "{seen:?}");
    }

    #[test]
    fn sets_and_removes_the_run_value() {
        let base = test_key("run");
        let cmd = r#""C:\Apps\Orbit App.exe""#;
        assert_eq!(run_value(&base.0), None);
        set_run_value(&base.0, Some(cmd)).unwrap();
        assert_eq!(run_value(&base.0).as_deref(), Some(cmd));
        set_run_value(&base.0, None).unwrap();
        assert_eq!(run_value(&base.0), None);
        // Removing what isn't there is fine.
        set_run_value(&base.0, None).unwrap();
    }

    #[test]
    fn the_uninstaller_removes_every_key_the_app_writes() {
        let hooks = include_str!("../../windows/hooks.nsh");
        let run = format!(r#"DeleteRegValue HKCU "{RUN_KEY}" "{RUN_VALUE}""#);
        assert!(hooks.contains(&run), "{run}");
        for (_, vendor) in BROWSERS {
            let key = format!(r#"DeleteRegKey HKCU "Software\{vendor}\NativeMessagingHosts\{}""#, crate::nativehost::HOST);
            assert!(hooks.contains(&key), "{key}");
        }
    }
}
