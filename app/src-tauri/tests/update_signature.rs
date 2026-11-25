//! A changed installer or a signature from another key is refused by the updater plugin's own
//! check (`Update::verify`, which download and install also run), with keys made for the test.
//! The real signing key is never used here.
use base64::Engine;
use tauri_plugin_updater::UpdaterExt;

const VERSION: &str = "99.0.0";
const INSTALLER: &[u8] = b"pretend this is Fishy Client_99.0.0_x64_en-US.msi";

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

struct TestKey {
    public: String,
    secret: minisign::SecretKey,
    pk: minisign::PublicKey,
}

fn test_key() -> TestKey {
    let minisign::KeyPair { pk, sk } = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    TestKey { public: b64(&pk.to_box().unwrap().into_string()), secret: sk, pk }
}

/// Signed the way the Tauri CLI signs a release: the version goes in the trusted comment.
fn sign(key: &TestKey, bytes: &[u8], version: &str) -> String {
    let comment = format!("timestamp:1790000000	file:Fishy Client.msi	version:{version}");
    let sig = minisign::sign(Some(&key.pk), &key.secret, std::io::Cursor::new(bytes), Some(&comment), Some("signature from tauri secret key")).unwrap();
    b64(&sig.into_string())
}

/// What the updater would check an installer against, trusting `pubkey`.
fn update(pubkey: &str, signature: &str) -> tauri_plugin_updater::Update {
    let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
    ctx.config_mut().plugins.0.insert("updater".into(), serde_json::json!({ "pubkey": pubkey }));
    let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(ctx).unwrap();
    let manifest = serde_json::json!({
        "version": VERSION,
        "platforms": { "test-target": { "url": "https://example.invalid/fishy.msi", "signature": signature } }
    });
    app.updater_builder()
        .target("test-target")
        .endpoints(vec!["https://example.invalid/latest.json".parse().unwrap()])
        .unwrap()
        .build()
        .unwrap()
        .restore_update(manifest)
        .unwrap()
        .expect("99.0.0 is an update")
}

#[test]
fn a_correctly_signed_installer_is_accepted() {
    let key = test_key();
    assert!(update(&key.public, &sign(&key, INSTALLER, VERSION)).verify(INSTALLER).is_ok());
}

#[test]
fn a_modified_installer_is_refused() {
    let key = test_key();
    let mut changed = INSTALLER.to_vec();
    changed[0] ^= 1;
    assert!(update(&key.public, &sign(&key, INSTALLER, VERSION)).verify(&changed).is_err());
}

#[test]
fn a_signature_from_another_key_is_refused() {
    let (trusted, other) = (test_key(), test_key());
    assert!(update(&trusted.public, &sign(&other, INSTALLER, VERSION)).verify(INSTALLER).is_err());
}

#[test]
fn an_old_release_offered_as_a_new_version_is_refused() {
    let key = test_key();
    assert!(update(&key.public, &sign(&key, INSTALLER, "1.0.0")).verify(INSTALLER).is_err());
}

#[test]
fn the_real_public_key_refuses_anything_it_did_not_sign() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let real = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(real).unwrap();
    assert!(String::from_utf8(decoded).unwrap().starts_with("untrusted comment: minisign public key"));
    let key = test_key();
    assert!(update(real, &sign(&key, INSTALLER, VERSION)).verify(INSTALLER).is_err());
}
