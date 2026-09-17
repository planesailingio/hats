//! Per-hat VS Code profiles.
//!
//! A VS Code profile holds its own `settings.json` while sharing extensions,
//! extension state, keybindings, snippets and tasks with the default profile.
//! That is exactly one hat's worth of editor configuration, so hats gives each
//! hat a profile named after it and lets VS Code remember which profile a
//! folder was last opened with.
//!
//! Profiles live in `storage.json` under `userDataProfiles`. VS Code reads
//! that file once at startup, keeps it in memory and rewrites the whole file
//! whenever anything changes, with no file watch. An entry added while VS Code
//! is running would therefore be overwritten, so every write here happens only
//! while VS Code is stopped, and [`running`] is checked again immediately
//! before the file is replaced.
//!
//! hats only ever removes what it created: its profiles are the ones whose
//! `location` is `hats-<hat>`. A profile the user made by hand with the same
//! name is left alone, and counts as that hat's profile.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

/// The `useDefaultFlags` every hat profile is created with: everything shared
/// with the default profile except the settings themselves.
const SHARED_WITH_DEFAULT: &[&str] = &[
    "keybindings",
    "snippets",
    "tasks",
    "extensions",
    "globalState",
    "mcp",
    "prompts",
    "languageModels",
];

/// The key holding the profile list in `storage.json`.
const PROFILES_KEY: &str = "userDataProfiles";
/// The key holding the folder-to-profile memory in `storage.json`.
const ASSOCIATIONS_KEY: &str = "profileAssociations";

/// The setting the hats extension reads to know which hat a profile is for.
const HAT_SETTING: &str = "hats.hat";

/// VS Code's data directory.
fn app_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Code")
    } else {
        home.join(".config").join("Code")
    }
}

/// The directory holding `settings.json`, the profiles and the state.
pub fn user_dir(home: &Path) -> PathBuf {
    app_dir(home).join("User")
}

/// Whether VS Code has been set up on this machine at all.
pub fn installed(home: &Path) -> bool {
    user_dir(home).is_dir()
}

/// VS Code's state file, which holds the profile list.
fn storage_path(home: &Path) -> PathBuf {
    user_dir(home).join("globalStorage").join("storage.json")
}

/// The `location` of the profile hats creates for `hat`. The `hats-` prefix
/// marks it as ours to remove, and keeps it clear of VS Code's own ids.
fn location(hat: &str) -> String {
    format!("hats-{hat}")
}

/// Where a hat's profile keeps its settings.
pub fn profile_dir(home: &Path, hat: &str) -> PathBuf {
    user_dir(home).join("profiles").join(location(hat))
}

/// The file VS Code writes its process id to at startup and deletes on exit.
fn lock_path(home: &Path) -> PathBuf {
    app_dir(home).join("code.lock")
}

/// Whether VS Code is running, so the state file must be left alone.
///
/// A lock file left behind by a crash names a process that has gone, which
/// reads as not running. A reused process id reads as running, which only
/// defers the change: the safe direction.
pub fn running(home: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(lock_path(home)) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    alive(pid)
}

/// Whether a process exists, via `kill -0`, which signals nothing.
fn alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Every profile name VS Code knows, hats-made or not.
pub fn profile_names(home: &Path) -> Result<Vec<String>> {
    let storage = read_storage(home)?;
    Ok(profiles(&storage)
        .iter()
        .filter_map(|p| p.get("name")?.as_str().map(str::to_string))
        .collect())
}

/// Whether VS Code already has a profile for this hat, however it was made.
pub fn has_profile(home: &Path, hat: &str) -> Result<bool> {
    Ok(profile_names(home)?.iter().any(|n| n == hat))
}

/// Whether the profile for this hat is one hats created, and so may remove.
pub fn owns_profile(home: &Path, hat: &str) -> Result<bool> {
    let storage = read_storage(home)?;
    Ok(profiles(&storage)
        .iter()
        .any(|p| p.get("location").and_then(Value::as_str) == Some(location(hat).as_str())))
}

/// Give this hat a VS Code profile: its own settings, everything else shared.
///
/// Returns whether it registered one. It does nothing when VS Code already
/// has a profile of that name, or when VS Code is running.
pub fn register(home: &Path, hat: &str) -> Result<bool> {
    if !installed(home) || running(home) || has_profile(home, hat)? {
        return Ok(false);
    }

    let dir = profile_dir(home, hat);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    owner_only(&dir)?;

    let settings = dir.join("settings.json");
    if settings.symlink_metadata().is_err() {
        std::fs::write(&settings, seeded_settings(home, hat))
            .with_context(|| format!("writing {}", settings.display()))?;
    }

    let mut storage = read_storage(home)?;
    let mut list = profiles(&storage).to_vec();
    let mut flags = Map::new();
    for flag in SHARED_WITH_DEFAULT {
        flags.insert((*flag).to_string(), Value::Bool(true));
    }
    list.push(json!({
        "location": location(hat),
        "name": hat,
        "useDefaultFlags": Value::Object(flags),
    }));
    storage.insert(PROFILES_KEY.into(), Value::Array(list));
    write_storage(home, &storage)?;
    Ok(true)
}

/// Take this hat's profile out of VS Code, along with any folders that would
/// reopen with it. The profile's own directory is moved to the backups by
/// `hats hat delete`, like the hat's other files.
///
/// Only a profile hats created is touched.
pub fn unregister(home: &Path, hat: &str) -> Result<()> {
    if !installed(home) {
        return Ok(());
    }
    anyhow::ensure!(
        !running(home),
        "VS Code is running, and would write its own profile list back over this change. \
         Quit VS Code first."
    );

    let ours = location(hat);
    let mut storage = read_storage(home)?;
    let list: Vec<Value> = profiles(&storage)
        .iter()
        .filter(|p| p.get("location").and_then(Value::as_str) != Some(ours.as_str()))
        .cloned()
        .collect();
    storage.insert(PROFILES_KEY.into(), Value::Array(list));

    // Folders remembered against the profile would otherwise name an id that
    // no longer exists.
    if let Some(Value::Object(assoc)) = storage.get_mut(ASSOCIATIONS_KEY) {
        for key in ["workspaces", "emptyWindows"] {
            if let Some(Value::Object(map)) = assoc.get_mut(key) {
                map.retain(|_, v| v.as_str() != Some(ours.as_str()));
            }
        }
    }

    write_storage(home, &storage)
}

/// Ask VS Code to quit as if from its menu: it prompts about unsaved files and
/// saves its window layout. Returns whether it has stopped.
pub fn quit(home: &Path, wait: Duration) -> Result<bool> {
    if !running(home) {
        return Ok(true);
    }
    if cfg!(target_os = "macos") {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "Visual Studio Code" to quit"#)
            .output()
            .context("asking VS Code to quit")?;
    } else {
        // No graceful equivalent to reach for on Linux.
        signal(home, "TERM")?;
    }
    Ok(wait_stopped(home, wait))
}

/// Kill VS Code outright. Its helper processes go with the main one.
pub fn force_stop(home: &Path, wait: Duration) -> Result<bool> {
    if !running(home) {
        return Ok(true);
    }
    signal(home, "KILL")?;
    Ok(wait_stopped(home, wait))
}

/// Send one signal to the process named in the lock file, and nothing else:
/// no pattern matching over the process list.
fn signal(home: &Path, sig: &str) -> Result<()> {
    let text = std::fs::read_to_string(lock_path(home))
        .with_context(|| format!("reading {}", lock_path(home).display()))?;
    let pid: u32 = text
        .trim()
        .parse()
        .with_context(|| format!("`{}` is not a process id", text.trim()))?;
    std::process::Command::new("kill")
        .arg(format!("-{sig}"))
        .arg(pid.to_string())
        .output()
        .with_context(|| format!("sending SIG{sig} to {pid}"))?;
    Ok(())
}

/// Poll until VS Code has stopped, or the wait runs out.
pub fn wait_stopped(home: &Path, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        if !running(home) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// A copy of the default settings, marked with the hat it belongs to so the
/// hats extension can tell which profile is which.
fn seeded_settings(home: &Path, hat: &str) -> String {
    let marker = format!("  \"{HAT_SETTING}\": \"{hat}\",");
    match std::fs::read_to_string(user_dir(home).join("settings.json")) {
        Ok(text) => match open_brace(&text) {
            // VS Code reads settings as JSON with comments, where a trailing
            // comma is allowed, so this holds even for an empty `{}`, and any
            // comments in the copy survive.
            Some(at) => format!(
                "{}\n{marker}\n{}",
                &text[..=at],
                text[at + 1..].trim_start_matches(['\n', '\r'])
            ),
            None => format!("{{\n{marker}\n}}\n"),
        },
        Err(_) => format!("{{\n  \"{HAT_SETTING}\": \"{hat}\"\n}}\n"),
    }
}

/// The index of the `{` that opens the settings object, skipping comments.
fn open_brace(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => return Some(i),
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += text[i..].find('\n').unwrap_or(text.len() - i);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += text[i..].find("*/").map_or(text.len() - i, |end| end + 2);
            }
            _ => i += 1,
        }
    }
    None
}

/// `storage.json` as a JSON object. A missing file reads as empty: VS Code
/// writes it on first run.
fn read_storage(home: &Path) -> Result<Map<String, Value>> {
    let path = storage_path(home);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", path.display()))?
    {
        Value::Object(map) => Ok(map),
        _ => anyhow::bail!("{} does not hold a JSON object", path.display()),
    }
}

/// The profile list, or nothing when VS Code has never made one.
fn profiles(storage: &Map<String, Value>) -> &[Value] {
    storage
        .get(PROFILES_KEY)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Replace `storage.json`, keeping a copy of what was there.
///
/// The write is atomic, as VS Code's own is: a complete file is put in place
/// by a rename, so a crash cannot leave VS Code without its state.
fn write_storage(home: &Path, storage: &Map<String, Value>) -> Result<()> {
    let path = storage_path(home);
    if path.is_file() {
        let backup = path.with_extension("json.hats-backup");
        std::fs::copy(&path, &backup)
            .with_context(|| format!("backing up {} to {}", path.display(), backup.display()))?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    // Checked as late as possible: VS Code may have started since the plan was
    // made, and its first save would undo this write.
    anyhow::ensure!(
        !running(home),
        "VS Code started while hats was writing its profile list; nothing changed"
    );

    let text = serde_json::to_string_pretty(&Value::Object(storage.clone()))?;
    let temp = path.with_extension("json.hats-tmp");
    std::fs::write(&temp, text).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, &path).with_context(|| format!("replacing {}", path.display()))
}

#[cfg(unix)]
fn owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting 0700 on {}", path.display()))
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home with VS Code set up and one profile of its own.
    fn home_with_vscode() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        let user = user_dir(home.path());
        std::fs::create_dir_all(user.join("globalStorage")).unwrap();
        std::fs::write(
            user.join("settings.json"),
            "// mine\n{\n  \"editor.fontSize\": 14\n}\n",
        )
        .unwrap();
        write_raw(
            home.path(),
            r#"{
  "theme": "dark",
  "userDataProfiles": [ { "location": "abc123", "name": "Work" } ]
}"#,
        );
        home
    }

    fn write_raw(home: &Path, text: &str) {
        let path = storage_path(home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read_raw(home: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(storage_path(home)).unwrap()).unwrap()
    }

    fn lock_with(home: &Path, pid: u32) {
        std::fs::create_dir_all(app_dir(home)).unwrap();
        std::fs::write(lock_path(home), pid.to_string()).unwrap();
    }

    /// A process id that is certainly gone: a child we waited for.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn a_hats_profile_is_named_after_the_hat_and_shares_all_but_settings() {
        let home = home_with_vscode();
        assert!(register(home.path(), "acme").unwrap());

        let stored = read_raw(home.path());
        let list = stored["userDataProfiles"].as_array().unwrap();
        assert_eq!(list.len(), 2, "the existing profile is kept");
        let ours = &list[1];
        assert_eq!(ours["name"], "acme");
        assert_eq!(ours["location"], "hats-acme");
        let flags = ours["useDefaultFlags"].as_object().unwrap();
        assert_eq!(flags.len(), SHARED_WITH_DEFAULT.len());
        assert!(flags.values().all(|v| v == true));
        assert!(!flags.contains_key("settings"), "settings are per profile");
        assert_eq!(stored["theme"], "dark", "other keys survive");
    }

    #[test]
    fn the_profile_starts_from_the_default_settings_and_says_which_hat_it_is() {
        let home = home_with_vscode();
        register(home.path(), "acme").unwrap();

        let text = std::fs::read_to_string(profile_dir(home.path(), "acme").join("settings.json"))
            .unwrap();
        assert!(text.contains(r#""hats.hat": "acme","#), "{text}");
        assert!(text.contains(r#""editor.fontSize": 14"#), "{text}");
        assert!(text.starts_with("// mine"), "comments survive: {text}");
        // Valid JSON with comments: strip them and the trailing comma is fine.
        assert!(text.find("hats.hat").unwrap() < text.find("fontSize").unwrap());
    }

    #[test]
    fn settings_are_seeded_even_with_no_default_file_or_an_empty_object() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(user_dir(home.path())).unwrap();
        register(home.path(), "solo").unwrap();
        let text = std::fs::read_to_string(profile_dir(home.path(), "solo").join("settings.json"))
            .unwrap();
        assert_eq!(text, "{\n  \"hats.hat\": \"solo\"\n}\n");

        std::fs::write(user_dir(home.path()).join("settings.json"), "{}").unwrap();
        register(home.path(), "empty").unwrap();
        let text = std::fs::read_to_string(profile_dir(home.path(), "empty").join("settings.json"))
            .unwrap();
        assert_eq!(text, "{\n  \"hats.hat\": \"empty\",\n}");
    }

    #[test]
    fn a_profile_that_already_exists_is_left_alone() {
        let home = home_with_vscode();
        // Same name, made by the user rather than by hats.
        write_raw(
            home.path(),
            r#"{ "userDataProfiles": [ { "location": "abc123", "name": "acme" } ] }"#,
        );
        assert!(!register(home.path(), "acme").unwrap());
        assert!(has_profile(home.path(), "acme").unwrap());
        assert!(!owns_profile(home.path(), "acme").unwrap());
        assert_eq!(
            read_raw(home.path())["userDataProfiles"][0]["location"],
            "abc123"
        );
    }

    #[test]
    fn registering_twice_changes_nothing() {
        let home = home_with_vscode();
        assert!(register(home.path(), "acme").unwrap());
        let after_first = std::fs::read_to_string(storage_path(home.path())).unwrap();
        assert!(!register(home.path(), "acme").unwrap());
        assert_eq!(
            std::fs::read_to_string(storage_path(home.path())).unwrap(),
            after_first
        );
    }

    #[test]
    fn nothing_is_written_while_vscode_is_running() {
        let home = home_with_vscode();
        lock_with(home.path(), std::process::id());
        assert!(running(home.path()));

        let before = std::fs::read_to_string(storage_path(home.path())).unwrap();
        assert!(!register(home.path(), "acme").unwrap());
        assert_eq!(
            std::fs::read_to_string(storage_path(home.path())).unwrap(),
            before
        );
        assert!(unregister(home.path(), "acme").is_err());
    }

    #[test]
    fn a_lock_left_by_a_crash_does_not_count_as_running() {
        let home = home_with_vscode();
        lock_with(home.path(), dead_pid());
        assert!(!running(home.path()));
        assert!(register(home.path(), "acme").unwrap());
    }

    #[test]
    fn unregistering_removes_only_the_hats_profile_and_its_folders() {
        let home = home_with_vscode();
        register(home.path(), "acme").unwrap();
        let mut storage = read_storage(home.path()).unwrap();
        storage.insert(
            ASSOCIATIONS_KEY.into(),
            json!({
                "workspaces": { "file:///a": "hats-acme", "file:///b": "abc123" },
                "emptyWindows": { "1": "hats-acme" }
            }),
        );
        write_storage(home.path(), &storage).unwrap();

        unregister(home.path(), "acme").unwrap();

        let stored = read_raw(home.path());
        let list = stored["userDataProfiles"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "Work", "the user's profile stays");
        assert_eq!(
            stored["profileAssociations"]["workspaces"]["file:///b"],
            "abc123"
        );
        assert!(
            stored["profileAssociations"]["workspaces"]
                .get("file:///a")
                .is_none()
        );
        assert_eq!(
            stored["profileAssociations"]["emptyWindows"]
                .as_object()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn the_previous_state_file_is_kept() {
        let home = home_with_vscode();
        let before = std::fs::read_to_string(storage_path(home.path())).unwrap();
        register(home.path(), "acme").unwrap();
        let backup = storage_path(home.path()).with_extension("json.hats-backup");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), before);
    }

    #[test]
    fn a_broken_state_file_is_an_error_and_is_left_as_it_is() {
        let home = home_with_vscode();
        write_raw(home.path(), "{ not json");
        assert!(register(home.path(), "acme").is_err());
        assert!(profile_names(home.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(storage_path(home.path())).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn a_home_without_vscode_registers_nothing() {
        let home = tempfile::tempdir().unwrap();
        assert!(!installed(home.path()));
        assert!(!register(home.path(), "acme").unwrap());
        unregister(home.path(), "acme").unwrap();
    }

    #[test]
    fn waiting_gives_up_when_vscode_stays_up_and_returns_when_it_goes() {
        let home = home_with_vscode();
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        lock_with(home.path(), child.id());
        assert!(!wait_stopped(home.path(), Duration::from_millis(300)));

        child.kill().unwrap();
        child.wait().unwrap();
        assert!(wait_stopped(home.path(), Duration::from_secs(5)));
    }

    #[cfg(unix)]
    #[test]
    fn the_profile_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = home_with_vscode();
        register(home.path(), "acme").unwrap();
        let mode = std::fs::metadata(profile_dir(home.path(), "acme"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }
}
