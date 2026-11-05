//! Verifies that the built installer's updater signature matches the public key
//! embedded in tauri.conf.json (i.e. installed copies would accept this update).
//! Skips when no signed bundle exists (run after `npm run tauri build`).

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

fn b64(s: &str) -> String {
    String::from_utf8(base64::engine::general_purpose::STANDARD.decode(s.trim()).expect("base64")).expect("utf8")
}

#[test]
fn installer_signature_matches_config_pubkey() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap()).unwrap();
    let version = conf["version"].as_str().unwrap();
    let exe = root.join(format!("target/release/bundle/nsis/Ycode_{version}_x64-setup.exe"));
    let sig = exe.with_extension("exe.sig");
    if !exe.exists() || !sig.exists() {
        eprintln!("no signed bundle at {}; skipping", exe.display());
        return;
    }
    let pk = PublicKey::decode(&b64(conf["plugins"]["updater"]["pubkey"].as_str().unwrap())).expect("public key");
    let signature = Signature::decode(&b64(&std::fs::read_to_string(&sig).unwrap())).expect("signature");
    let data = std::fs::read(&exe).unwrap();
    pk.verify(&data, &signature, false).expect("installer signature must verify against tauri.conf.json pubkey");
    // Tampered data must fail.
    let mut bad = data.clone();
    bad[100] ^= 0xff;
    assert!(pk.verify(&bad, &signature, false).is_err());
}
