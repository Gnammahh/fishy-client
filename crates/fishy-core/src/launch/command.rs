//! Builds the game's command line, in the same order as Legendary `get_launch_parameters` (spec §6d).
use super::LaunchError;
use crate::auth::Secret;
use crate::install::store::{file_path, InstalledGame};
use crate::library::Game;
use std::fmt;
use std::path::{Path, PathBuf};

/// The program to start, its folder and its arguments. `Debug` hides the one-time code.
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub working_dir: PathBuf,
    pub args: Vec<String>,
}

const AUTH_PASSWORD: &str = "-AUTH_PASSWORD=";

impl fmt::Debug for LaunchCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args: Vec<String> = self
            .args
            .iter()
            .map(|a| if a.starts_with(AUTH_PASSWORD) && a.len() > AUTH_PASSWORD.len() { format!("{AUTH_PASSWORD}[redacted]") } else { a.clone() })
            .collect();
        f.debug_struct("LaunchCommand").field("program", &self.program).field("working_dir", &self.working_dir).field("args", &args).finish()
    }
}

/// Everything the command line is built from. `exchange_code` None means offline.
pub struct LaunchInputs<'a> {
    pub installed: &'a InstalledGame,
    pub game: &'a Game,
    pub display_name: &'a str,
    pub account_id: &'a str,
    pub locale: &'a str,
    pub exchange_code: Option<&'a Secret>,
    pub ovt_path: Option<&'a Path>,
}

/// Splits on spaces; a part in double quotes stays one argument, with the quotes removed.
pub fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut has_part = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has_part = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has_part {
                    out.push(std::mem::take(&mut current));
                    has_part = false;
                }
            }
            c => {
                current.push(c);
                has_part = true;
            }
        }
    }
    if has_part {
        out.push(current);
    }
    out
}

/// "en-US" → "en". Unknown → "en".
pub fn language_from(tag: Option<&str>) -> String {
    tag.and_then(|t| t.split(['-', '_']).next())
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "en".into())
}

pub fn build_command(i: &LaunchInputs) -> Result<LaunchCommand, LaunchError> {
    if let Some(store) = &i.game.launch.third_party_store {
        return Err(LaunchError::ThirdParty(store_name(store)));
    }
    let exe = i.installed.executable.trim().trim_start_matches(['/', '\\']);
    let program = file_path(&i.installed.install_path, exe).map_err(|_| LaunchError::MissingProgram)?;
    if !program.is_file() {
        return Err(LaunchError::MissingProgram);
    }
    let working_dir = program.parent().map(Path::to_path_buf).unwrap_or_else(|| i.installed.install_path.clone());

    let mut args = split_args(&i.installed.launch_parameters);
    if let Some(extra) = &i.game.launch.additional_command_line {
        args.extend(split_args(extra));
    }
    let code = i.exchange_code.map(Secret::expose).unwrap_or("");
    args.extend([
        "-AUTH_LOGIN=unused".to_string(),
        format!("{AUTH_PASSWORD}{code}"),
        "-AUTH_TYPE=exchangecode".into(),
        format!("-epicapp={}", i.installed.app_name),
        "-epicenv=Prod".into(),
    ]);
    if let Some(ovt) = i.ovt_path {
        args.push(format!("-epicovt={}", ovt.display()));
    }
    args.extend([
        "-EpicPortal".to_string(),
        format!("-epicusername={}", i.display_name),
        format!("-epicuserid={}", i.account_id),
        format!("-epiclocale={}", i.locale),
        format!("-epicsandboxid={}", i.game.namespace),
    ]);
    Ok(LaunchCommand { program, working_dir, args })
}

/// Friendlier names for Epic's store ids.
fn store_name(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "origin" | "the ea app" => "the EA app".into(),
        "ubisoftconnect" | "uplay" => "Ubisoft Connect".into(),
        _ => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LaunchInfo;
    use chrono::Utc;

    fn installed(root: &Path, exe: &str, params: &str) -> InstalledGame {
        InstalledGame {
            app_name: "Reef".into(),
            title: "Reef Run".into(),
            version: "1".into(),
            install_path: root.to_path_buf(),
            executable: exe.into(),
            launch_parameters: params.into(),
            install_size: 1,
            installed_at: Utc::now(),
            last_played: None,
            parts: None,
            parent: None,
            namespace: String::new(),
            catalog_item_id: String::new(),
        }
    }

    fn game(launch: LaunchInfo) -> Game {
        Game {
            app_name: "Reef".into(),
            namespace: "ns1".into(),
            catalog_item_id: "cat1".into(),
            title: "Reef Run".into(),
            cover_url: None,
            installable: true,
            launch,
        }
    }

    fn with_exe(rel: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"MZ").unwrap();
        dir
    }

    fn inputs<'a>(inst: &'a InstalledGame, g: &'a Game, code: Option<&'a Secret>, ovt: Option<&'a Path>) -> LaunchInputs<'a> {
        LaunchInputs { installed: inst, game: g, display_name: "FishFan", account_id: "acc123", locale: "en", exchange_code: code, ovt_path: ovt }
    }

    #[test]
    fn split_args_keeps_quoted_parts_together() {
        assert_eq!(split_args(r#" -a  "-b c" d"e f"g "" "#), vec!["-a", "-b c", "de fg", ""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn builds_the_full_command_in_legendarys_order() {
        let dir = with_exe("Bin/Win64/Reef.exe");
        let inst = installed(dir.path(), "Bin/Win64/Reef.exe", "-windowed \"-log file\"");
        let g = game(LaunchInfo { additional_command_line: Some("-nosplash".into()), requires_ot: true, ..LaunchInfo::default() });
        let code = Secret::new("CODE123");
        let ovt = PathBuf::from(r"C:\Temp\FishyClient\ns1cat1.ovt");
        let cmd = build_command(&inputs(&inst, &g, Some(&code), Some(&ovt))).unwrap();
        assert_eq!(cmd.program, dir.path().join("Bin").join("Win64").join("Reef.exe"));
        assert_eq!(cmd.working_dir, dir.path().join("Bin").join("Win64"));
        assert_eq!(
            cmd.args,
            vec![
                "-windowed",
                "-log file",
                "-nosplash",
                "-AUTH_LOGIN=unused",
                "-AUTH_PASSWORD=CODE123",
                "-AUTH_TYPE=exchangecode",
                "-epicapp=Reef",
                "-epicenv=Prod",
                r"-epicovt=C:\Temp\FishyClient\ns1cat1.ovt",
                "-EpicPortal",
                "-epicusername=FishFan",
                "-epicuserid=acc123",
                "-epiclocale=en",
                "-epicsandboxid=ns1",
            ]
        );
    }

    #[test]
    fn offline_has_an_empty_password_and_no_ovt() {
        let dir = with_exe("Reef.exe");
        let inst = installed(dir.path(), "Reef.exe", "");
        let g = game(LaunchInfo::default());
        let cmd = build_command(&inputs(&inst, &g, None, None)).unwrap();
        assert_eq!(cmd.args[0], "-AUTH_LOGIN=unused");
        assert!(cmd.args.contains(&"-AUTH_PASSWORD=".to_string()));
        assert!(!cmd.args.iter().any(|a| a.starts_with("-epicovt")));
    }

    #[test]
    fn executable_with_leading_slash_or_backslashes_stays_inside() {
        let dir = with_exe("Game/Reef.exe");
        for exe in ["/Game/Reef.exe", r"Game\Reef.exe", r"\Game\Reef.exe"] {
            let inst = installed(dir.path(), exe, "");
            let cmd = build_command(&inputs(&inst, &game(LaunchInfo::default()), None, None)).unwrap();
            assert_eq!(cmd.program, dir.path().join("Game").join("Reef.exe"), "{exe}");
        }
    }

    #[test]
    fn unsafe_or_missing_program_is_refused() {
        let dir = with_exe("Reef.exe");
        let g = game(LaunchInfo::default());
        for exe in ["../outside.exe", "C:/Windows/notepad.exe", "", "Missing.exe"] {
            let inst = installed(dir.path(), exe, "");
            assert_eq!(build_command(&inputs(&inst, &g, None, None)), Err(LaunchError::MissingProgram), "{exe}");
        }
    }

    #[test]
    fn third_party_store_games_are_refused_with_a_friendly_name() {
        let dir = with_exe("Reef.exe");
        let inst = installed(dir.path(), "Reef.exe", "");
        let g = game(LaunchInfo { third_party_store: Some("Origin".into()), ..LaunchInfo::default() });
        assert_eq!(build_command(&inputs(&inst, &g, None, None)), Err(LaunchError::ThirdParty("the EA app".into())));
        let g = game(LaunchInfo { third_party_store: Some("UbisoftConnect".into()), ..LaunchInfo::default() });
        assert_eq!(build_command(&inputs(&inst, &g, None, None)), Err(LaunchError::ThirdParty("Ubisoft Connect".into())));
    }

    #[test]
    fn debug_output_hides_the_code() {
        let dir = with_exe("Reef.exe");
        let inst = installed(dir.path(), "Reef.exe", "");
        let g = game(LaunchInfo::default());
        let code = Secret::new("SUPERSECRETCODE");
        let cmd = build_command(&inputs(&inst, &g, Some(&code), None)).unwrap();
        let shown = format!("{cmd:?}");
        assert!(!shown.contains("SUPERSECRETCODE"), "{shown}");
        assert!(shown.contains("-AUTH_PASSWORD=[redacted]"));
    }

    #[test]
    fn language_is_the_first_part_of_the_tag() {
        assert_eq!(language_from(Some("en-US")), "en");
        assert_eq!(language_from(Some("de_DE")), "de");
        assert_eq!(language_from(Some("IT")), "it");
        assert_eq!(language_from(None), "en");
        assert_eq!(language_from(Some("")), "en");
    }
}
