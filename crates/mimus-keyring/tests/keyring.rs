use mimus_keyring::KeyringSecrets;
use mimus_secret::Secrets;

#[test]
fn default_service_and_missing_lookup() {
    assert_eq!(KeyringSecrets::default().service, "mimus");
    let s = KeyringSecrets { service: "mimus-test".into() };
    assert_eq!(s.get("mimus-test-definitely-missing"), None);
}
