//! Starting an installed game signed in as the user (spec §6d).
mod client;
mod command;
mod error;

pub use client::LaunchClient;
pub use command::{build_command, language_from, split_args, LaunchCommand, LaunchInputs};
pub use error::LaunchError;

use crate::auth::Secret;
use crate::install::store::InstalledGame;
use crate::library::Game;
use std::path::{Path, PathBuf};

/// Who is playing. Comes from the saved session, so it also works offline.
pub struct Player<'a> {
    pub display_name: &'a str,
    pub account_id: &'a str,
    pub locale: &'a str,
}

/// Checks the game can start, then (online) gets a fresh code and, if needed, the ownership token
/// (saved in `ovt_dir`), and builds the command. `token` None means offline: Epic isn't asked.
pub async fn prepare_launch(
    client: &LaunchClient,
    token: Option<&Secret>,
    player: &Player<'_>,
    installed: &InstalledGame,
    game: &Game,
    ovt_dir: &Path,
) -> Result<LaunchCommand, LaunchError> {
    let inputs = |code, ovt| LaunchInputs {
        installed,
        game,
        display_name: player.display_name,
        account_id: player.account_id,
        locale: player.locale,
        exchange_code: code,
        ovt_path: ovt,
    };
    // Check the program and store first, so a broken install doesn't use up a code.
    let offline_cmd = build_command(&inputs(None, None))?;
    let Some(token) = token else {
        return Ok(offline_cmd);
    };
    let code = client.exchange_code(token).await?;
    let mut ovt_path = None;
    if game.launch.requires_ot {
        let bytes = client.ownership_token(token, player.account_id, &game.namespace, &game.catalog_item_id).await?;
        let file = ovt_dir.join(format!("{}{}.ovt", game.namespace, game.catalog_item_id));
        std::fs::create_dir_all(ovt_dir)
            .and_then(|_| std::fs::write(&file, bytes))
            .map_err(|e| LaunchError::Spawn(format!("couldn't save a file it needs ({})", plain_reason(&e))))?;
        ovt_path = Some(file);
    }
    build_command(&inputs(Some(&code), ovt_path.as_deref()))
}

/// Starts the game and doesn't wait for it.
pub fn start(cmd: &LaunchCommand) -> Result<(), LaunchError> {
    use std::process::{Command, Stdio};
    const ERROR_ELEVATION_REQUIRED: i32 = 740;
    let result = Command::new(&cmd.program)
        .args(&cmd.args)
        .current_dir(&cmd.working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match result {
        Ok(_child) => Ok(()), // dropping the handle doesn't stop the game
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => Err(LaunchError::NeedsAdmin),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(LaunchError::MissingProgram),
        Err(e) => Err(LaunchError::Spawn(plain_reason(&e))),
    }
}

/// Windows' own wording without the trailing "(os error N)" or full stop.
fn plain_reason(e: &std::io::Error) -> String {
    let text = e.to_string();
    let text = text.split(" (os error").next().unwrap_or(&text);
    text.trim().trim_end_matches('.').to_string()
}

/// Deletes ownership token files left from earlier launches. Other files are left alone.
pub fn clean_ovt_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("ovt")) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// `%TEMP%\FishyClient`, where ownership token files go.
pub fn default_ovt_dir() -> PathBuf {
    std::env::temp_dir().join("FishyClient")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LaunchInfo;
    use chrono::Utc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const PLAYER: Player<'static> = Player { display_name: "FishFan", account_id: "acc123", locale: "en" };

    fn setup(launch: LaunchInfo) -> (tempfile::TempDir, InstalledGame, Game) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("game")).unwrap();
        std::fs::write(dir.path().join("game").join("Reef.exe"), b"MZ").unwrap();
        let inst = InstalledGame {
            app_name: "Reef".into(),
            title: "Reef Run".into(),
            version: "1".into(),
            install_path: dir.path().join("game"),
            executable: "Reef.exe".into(),
            launch_parameters: String::new(),
            install_size: 1,
            installed_at: Utc::now(),
            last_played: None,
            parts: None,
            parent: None,
            namespace: String::new(),
            catalog_item_id: String::new(),
        };
        let game = Game {
            app_name: "Reef".into(),
            namespace: "ns1".into(),
            catalog_item_id: "cat1".into(),
            title: "Reef Run".into(),
            cover_url: None,
            installable: true,
            launch,
        };
        (dir, inst, game)
    }

    async fn epic(ot: bool) -> (MockServer, LaunchClient) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/api/oauth/exchange"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"code": "CODE1"})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/ecommerceintegration/api/public/platforms/EPIC/identities/acc123/ownershipToken"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"OVTBYTES".to_vec()))
            .expect(u64::from(ot))
            .mount(&server)
            .await;
        let c = LaunchClient::with_base_urls(&server.uri(), &server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        (server, c)
    }

    #[tokio::test]
    async fn online_with_ownership_token_writes_the_file_and_adds_the_flag() {
        let (dir, inst, game) = setup(LaunchInfo { requires_ot: true, ..LaunchInfo::default() });
        let (_server, c) = epic(true).await;
        let ovt_dir = dir.path().join("tmp");
        let cmd = prepare_launch(&c, Some(&Secret::new("eg1~A")), &PLAYER, &inst, &game, &ovt_dir).await.unwrap();
        let ovt = ovt_dir.join("ns1cat1.ovt");
        assert_eq!(std::fs::read(&ovt).unwrap(), b"OVTBYTES");
        assert!(cmd.args.contains(&"-AUTH_PASSWORD=CODE1".to_string()));
        assert!(cmd.args.contains(&format!("-epicovt={}", ovt.display())));
    }

    #[tokio::test]
    async fn online_without_ownership_token_skips_that_call() {
        let (dir, inst, game) = setup(LaunchInfo::default());
        let (_server, c) = epic(false).await;
        let cmd = prepare_launch(&c, Some(&Secret::new("eg1~A")), &PLAYER, &inst, &game, &dir.path().join("tmp")).await.unwrap();
        assert!(!cmd.args.iter().any(|a| a.starts_with("-epicovt")));
    }

    #[tokio::test]
    async fn offline_never_asks_epic() {
        let (dir, inst, game) = setup(LaunchInfo { requires_ot: true, ..LaunchInfo::default() });
        let c = LaunchClient::with_base_urls("http://127.0.0.1:9", "http://127.0.0.1:9").with_retry_base(std::time::Duration::from_millis(1));
        let cmd = prepare_launch(&c, None, &PLAYER, &inst, &game, &dir.path().join("tmp")).await.unwrap();
        assert!(cmd.args.contains(&"-AUTH_PASSWORD=".to_string()));
    }

    #[tokio::test]
    async fn no_internet_is_a_network_error() {
        let (dir, inst, game) = setup(LaunchInfo::default());
        let c = LaunchClient::with_base_urls("http://127.0.0.1:9", "http://127.0.0.1:9").with_retry_base(std::time::Duration::from_millis(1));
        let err = prepare_launch(&c, Some(&Secret::new("eg1~A")), &PLAYER, &inst, &game, dir.path()).await.unwrap_err();
        assert_eq!(err, LaunchError::Network);
    }

    #[tokio::test]
    async fn missing_program_is_found_before_asking_epic() {
        let (dir, mut inst, game) = setup(LaunchInfo::default());
        inst.executable = "Gone.exe".into();
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
        let c = LaunchClient::with_base_urls(&server.uri(), &server.uri()).with_retry_base(std::time::Duration::from_millis(1));
        let err = prepare_launch(&c, Some(&Secret::new("eg1~A")), &PLAYER, &inst, &game, dir.path()).await.unwrap_err();
        assert_eq!(err, LaunchError::MissingProgram);
    }

    #[test]
    fn clean_removes_only_ovt_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.ovt"), b"x").unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
        clean_ovt_dir(dir.path());
        assert!(!dir.path().join("a.ovt").exists());
        assert!(dir.path().join("keep.txt").exists());
        clean_ovt_dir(&dir.path().join("does-not-exist")); // no panic
    }

    #[test]
    fn start_reports_a_missing_program() {
        let cmd = LaunchCommand { program: PathBuf::from(r"C:\definitely\not\here.exe"), working_dir: std::env::temp_dir(), args: vec![] };
        assert_eq!(start(&cmd), Err(LaunchError::MissingProgram));
    }

    #[cfg(windows)]
    #[test]
    fn start_runs_a_real_program_without_waiting() {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let cmd = LaunchCommand {
            program: PathBuf::from(system).join("System32").join("cmd.exe"),
            working_dir: std::env::temp_dir(),
            args: vec!["/c".into(), "exit".into()],
        };
        assert_eq!(start(&cmd), Ok(()));
    }
}
