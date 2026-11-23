//! Which games started by Play are still open (spec §6f.4). Program paths are only compared in
//! memory here: they are never logged, saved or sent anywhere.
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Lower-case folder names, with `/` and `\` alike and Windows' `\\?\` prefix dropped.
fn parts(p: &Path) -> Vec<String> {
    let text = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.split('\\').filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// True when `program` sits in `folder` or one of its subfolders (whole folder names only).
pub fn is_inside(program: &Path, folder: &Path) -> bool {
    let (prog, dir) = (parts(program), parts(folder));
    !dir.is_empty() && prog.len() > dir.len() && prog.starts_with(&dir)
}

/// Games started by Play that still have a program open: (code name, install folder, when added).
/// The most one check adds to a game's playtime: more means the PC slept in between (spec §6p).
pub const MAX_STEP: std::time::Duration = std::time::Duration::from_secs(15);

/// A watched game: code name, folder, when Play added it, when it was last seen, time counted.
#[derive(Debug)]
struct Watched {
    app: String,
    folder: PathBuf,
    added: Instant,
    seen: Instant,
    played: std::time::Duration,
}

#[derive(Debug, Default)]
pub struct RunningList {
    games: Vec<Watched>,
}

impl RunningList {
    /// True when the game wasn't in the list yet.
    pub fn add(&mut self, app_name: &str, folder: &Path) -> bool {
        if self.contains(app_name) {
            return false;
        }
        let now = Instant::now();
        self.games.push(Watched { app: app_name.to_string(), folder: folder.to_path_buf(), added: now, seen: now, played: std::time::Duration::ZERO });
        true
    }

    pub fn contains(&self, app_name: &str) -> bool {
        self.games.iter().any(|g| g.app == app_name)
    }

    /// The running code names, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut n: Vec<String> = self.games.iter().map(|g| g.app.clone()).collect();
        n.sort();
        n
    }

    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    /// Removes the games with no program open from their folder and returns their code names
    /// with how long they ran (spec §6p). `taken_at` is when `programs` was listed: a game Play
    /// added after that is kept for the next check, since the list couldn't include it.
    pub fn check(&mut self, programs: &[PathBuf], taken_at: Instant) -> Vec<(String, std::time::Duration)> {
        let mut closed = Vec::new();
        self.games.retain_mut(|g| {
            if g.added > taken_at {
                return true;
            }
            // Counted check by check, at most MAX_STEP each, so a PC asleep in between adds nothing.
            g.played += taken_at.saturating_duration_since(g.seen).min(MAX_STEP);
            g.seen = taken_at;
            let open = programs.iter().any(|p| is_inside(p, &g.folder));
            if !open {
                closed.push((g.app.clone(), g.played));
            }
            open
        });
        closed
    }
}

/// Play refuses a game that's already running.
pub fn already_running(title: &str) -> String {
    format!("{title} is already running.")
}

/// Update and Uninstall refuse a running game.
pub fn close_first(title: &str) -> String {
    format!("Close {title} first.")
}

/// The file paths of every open program. Uses Windows' "limited information" query, which also
/// works for elevated and anti-cheat-protected games (their memory can't be read, but their path
/// can). Programs that have ended (no threads left) are skipped.
#[derive(Default)]
pub struct OpenPrograms;

impl OpenPrograms {
    pub fn new() -> Self {
        OpenPrograms
    }

    /// Programs whose path Windows won't tell are skipped. No list at all means none open, so a
    /// card never stays stuck on Running.
    pub fn paths(&mut self) -> Vec<PathBuf> {
        windows_programs::paths()
    }
}

#[cfg(windows)]
mod windows_programs {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

    pub fn paths() -> Vec<PathBuf> {
        let mut out = Vec::new();
        // SAFETY: plain Win32 calls; every handle opened here is closed before returning, and the
        // buffers passed are sized as the calls require.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snap, &mut entry) != 0;
            while more {
                if entry.cntThreads > 0 && entry.th32ProcessID != 0 {
                    if let Some(p) = path_of(entry.th32ProcessID) {
                        out.push(p);
                    }
                }
                more = Process32NextW(snap, &mut entry) != 0;
            }
            CloseHandle(snap);
        }
        out
    }

    unsafe fn path_of(pid: u32) -> Option<PathBuf> {
        // SAFETY: see `paths`.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 32_768];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(handle);
            (ok && len > 0).then(|| PathBuf::from(OsString::from_wide(&buf[..len as usize])))
        }
    }
}

#[cfg(not(windows))]
mod windows_programs {
    pub fn paths() -> Vec<std::path::PathBuf> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_program_inside_the_folder_counts_a_look_alike_does_not() {
        let f = Path::new(r"C:\Games\Kelp");
        assert!(is_inside(Path::new(r"C:\Games\Kelp\kelp.exe"), f));
        assert!(is_inside(Path::new(r"c:\games\KELP\Binaries\Win64\kelp.exe"), f));
        assert!(is_inside(Path::new("C:/Games/Kelp/bin/kelp.exe"), f));
        assert!(is_inside(Path::new(r"C:\Games\Kelp\kelp.exe"), Path::new(r"C:\games\kelp\")));
        assert!(!is_inside(Path::new(r"C:\Games\Kelp2\kelp.exe"), f));
        assert!(!is_inside(Path::new(r"C:\Games\kelp.exe"), f));
        assert!(!is_inside(Path::new(r"D:\Games\Kelp\kelp.exe"), f));
    }

    #[test]
    fn check_removes_only_games_with_nothing_open() {
        let mut list = RunningList::default();
        assert!(list.add("kelp", Path::new(r"C:\Games\Kelp")));
        assert!(!list.add("kelp", Path::new(r"C:\Games\Kelp")));
        list.add("reef", Path::new(r"C:\Games\Reef"));
        let open = vec![PathBuf::from(r"C:\Games\Kelp\Binaries\Win64\kelp.exe"), PathBuf::from(r"C:\Windows\explorer.exe")];
        assert_eq!(names_of(list.check(&open, Instant::now())), vec!["reef".to_string()]);
        assert_eq!(list.names(), vec!["kelp".to_string()]);
        assert!(list.contains("kelp") && !list.contains("reef"));
        assert_eq!(names_of(list.check(&[], Instant::now())), vec!["kelp".to_string()]);
        assert!(list.is_empty());
    }

    #[test]
    fn a_game_added_after_the_list_was_taken_is_kept() {
        let mut list = RunningList::default();
        let taken = Instant::now();
        list.add("kelp", Path::new(r"C:\Games\Kelp"));
        assert!(list.check(&[], taken).is_empty(), "the list was taken before Play added it");
        assert_eq!(names_of(list.check(&[], Instant::now())), vec!["kelp".to_string()]);
    }

    fn names_of(closed: Vec<(String, std::time::Duration)>) -> Vec<String> {
        closed.into_iter().map(|(n, _)| n).collect()
    }

    #[test]
    fn closed_games_report_how_long_they_ran() {
        let mut list = RunningList::default();
        list.add("kelp", Path::new("C:/Games/Kelp"));
        let open = vec![PathBuf::from("C:/Games/Kelp/kelp.exe")];
        let start = Instant::now();
        // A check every 5 seconds for 90 seconds, then the game is gone.
        for n in 1..18 {
            assert!(list.check(&open, start + std::time::Duration::from_secs(5 * n)).is_empty());
        }
        let closed = list.check(&[], start + std::time::Duration::from_secs(90));
        assert_eq!(closed.len(), 1);
        assert!(closed[0].1 >= std::time::Duration::from_secs(89) && closed[0].1 <= std::time::Duration::from_secs(91), "{:?}", closed[0].1);
    }

    #[test]
    fn a_long_gap_between_checks_counts_only_a_short_step() {
        // The PC slept overnight with the game open: those hours aren't playtime.
        let mut list = RunningList::default();
        list.add("kelp", Path::new("C:/Games/Kelp"));
        let open = vec![PathBuf::from("C:/Games/Kelp/kelp.exe")];
        let t0 = Instant::now() + std::time::Duration::from_secs(5);
        assert!(list.check(&open, t0).is_empty());
        let after_sleep = t0 + std::time::Duration::from_secs(8 * 3600);
        let closed = list.check(&[], after_sleep);
        assert!(closed[0].1 <= std::time::Duration::from_secs(6) + MAX_STEP, "{:?}", closed[0].1);
    }

    #[test]
    fn messages_match_the_spec() {
        assert_eq!(already_running("Kelp Runner"), "Kelp Runner is already running.");
        assert_eq!(close_first("Kelp Runner"), "Close Kelp Runner first.");
    }

    #[test]
    fn program_paths_are_never_empty() {
        let paths = OpenPrograms::new().paths();
        assert!(paths.len() > 5);
        assert!(paths.iter().all(|p| !p.as_os_str().is_empty() && p.is_absolute()));
    }

    #[test]
    fn open_programs_include_this_test() {
        let me = std::env::current_exe().unwrap();
        let paths = OpenPrograms::new().paths();
        assert!(paths.iter().any(|p| is_inside(p, me.parent().unwrap())));
    }
}
