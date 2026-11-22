//! Which games started by Play are still open (spec §6f.4). Program paths are only compared in
//! memory here: they are never logged, saved or sent anywhere.
use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

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

/// Games started by Play that still have a program open: (code name, install folder).
#[derive(Debug, Default)]
pub struct RunningList {
    games: Vec<(String, PathBuf)>,
}

impl RunningList {
    /// True when the game wasn't in the list yet.
    pub fn add(&mut self, app_name: &str, folder: &Path) -> bool {
        if self.contains(app_name) {
            return false;
        }
        self.games.push((app_name.to_string(), folder.to_path_buf()));
        true
    }

    pub fn contains(&self, app_name: &str) -> bool {
        self.games.iter().any(|(a, _)| a == app_name)
    }

    /// The running code names, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut n: Vec<String> = self.games.iter().map(|(a, _)| a.clone()).collect();
        n.sort();
        n
    }

    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    /// Removes the games with no program open from their folder and returns their code names.
    pub fn check(&mut self, programs: &[PathBuf]) -> Vec<String> {
        let mut closed = Vec::new();
        self.games.retain(|(app, folder)| {
            let open = programs.iter().any(|p| is_inside(p, folder));
            if !open {
                closed.push(app.clone());
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

/// The file paths of every open program. Kept between checks, so a program's path is only asked
/// for once.
pub struct OpenPrograms {
    /// None when Windows wouldn't give the list; asked again on the next check.
    system: Option<System>,
}

impl Default for OpenPrograms {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenPrograms {
    pub fn new() -> Self {
        Self { system: System::new().ok() }
    }

    /// Programs whose path Windows won't tell are skipped. No list at all means none open, so a
    /// card never stays stuck on Running.
    pub fn paths(&mut self) -> Vec<PathBuf> {
        if self.system.is_none() {
            self.system = System::new().ok();
        }
        let Some(system) = self.system.as_mut() else { return Vec::new() };
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
        system.processes().values().filter_map(|p| p.exe().map(Path::to_path_buf)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(list.check(&open), vec!["reef".to_string()]);
        assert_eq!(list.names(), vec!["kelp".to_string()]);
        assert!(list.contains("kelp") && !list.contains("reef"));
        assert_eq!(list.check(&[]), vec!["kelp".to_string()]);
        assert!(list.is_empty());
    }

    #[test]
    fn messages_match_the_spec() {
        assert_eq!(already_running("Kelp Runner"), "Kelp Runner is already running.");
        assert_eq!(close_first("Kelp Runner"), "Close Kelp Runner first.");
    }

    #[test]
    fn open_programs_include_this_test() {
        let me = std::env::current_exe().unwrap();
        let paths = OpenPrograms::new().paths();
        assert!(paths.iter().any(|p| is_inside(p, me.parent().unwrap())));
    }
}
