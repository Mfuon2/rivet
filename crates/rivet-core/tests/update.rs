use rivet_core::update::{self, Asset, Release, Version};
use serde_json::json;
use sha2::{Digest, Sha256};

#[test]
fn numeric_versions_do_not_compare_lexicographically_or_accept_tags_as_commands() {
    assert!(Version::parse("0.1.10").unwrap() > Version::parse("v0.1.9").unwrap());
    assert!(Version::parse("1.0.0").unwrap() > Version::parse("0.99.99").unwrap());
    for invalid in [
        "0.1",
        "0.1.1.1",
        "0.1.-1",
        "0.1.01",
        "0.1.2/../../",
        "0.1.2;echo",
        "0.1.2-beta",
        "18446744073709551616.0.0",
    ] {
        assert!(Version::parse(invalid).is_err(), "{invalid}");
    }
}
#[test]
fn latest_published_release_includes_previews_but_never_drafts_or_downgrades() {
    let data = serde_json::to_vec(&json!([
        {"tag_name":"v0.1.9","assets":[]},
        {"tag_name":"v0.1.10","prerelease":true,"assets":[]},
        {"tag_name":"v9.0.0","draft":true,"assets":[]},
        {"tag_name":"v999.bad.0","assets":[]}
    ]))
    .unwrap();
    assert_eq!(
        update::select_release(&data, "0.1.8")
            .unwrap()
            .unwrap()
            .version(),
        "0.1.10"
    );
    assert!(update::select_release(&data, "0.1.10").unwrap().is_none());
    assert!(update::select_release(&data, "1.0.0").unwrap().is_none());
}
#[test]
fn release_assets_must_belong_to_exact_repository_version_and_filename() {
    let name = "rivet-0.1.3-macos-aarch64.zip";
    let url = format!("https://github.com/Mfuon2/rivet/releases/download/v0.1.3/{name}");
    let mut release = Release {
        tag_name: "v0.1.3".into(),
        draft: false,
        prerelease: true,
        body: None,
        assets: vec![Asset {
            name: name.into(),
            browser_download_url: url.clone(),
            size: 100,
        }],
    };
    assert!(release.asset(name).is_ok());
    for bad in [
        url.replace("Mfuon2/rivet", "attacker/rivet"),
        url.replace("https:", "http:"),
        url.replace("v0.1.3", "v0.1.2"),
    ] {
        release.assets[0].browser_download_url = bad;
        assert!(release.asset(name).is_err());
    }
    release.assets[0].browser_download_url = url;
    release.assets.push(release.assets[0].clone());
    assert!(release.asset(name).is_err());
}
#[test]
fn pinned_minisign_key_verifies_real_release_and_rejects_tampering() {
    let manifest = include_bytes!("fixtures/update/SHA256SUMS");
    let signature = include_str!("fixtures/update/SHA256SUMS.minisig");
    assert_eq!(
        update::authenticated_checksums(manifest, signature)
            .unwrap()
            .len(),
        9
    );
    let mut tampered = manifest.to_vec();
    tampered[0] = if tampered[0] == b'a' { b'b' } else { b'a' };
    assert!(update::authenticated_checksums(&tampered, signature).is_err());
    assert!(
        update::authenticated_checksums(manifest, &signature.replace("Rivet 0.1.3", "Rivet 0.1.4"))
            .is_err()
    );
    assert!(update::authenticated_checksums(manifest, "not a signature").is_err());
}
#[test]
fn checksums_reject_unsafe_duplicate_or_malformed_names() {
    let hash = "a".repeat(64);
    for bad in [
        format!("{hash}  ../app.exe\n"),
        format!("{hash}  /app.exe\n"),
        format!("{hash}  app.exe\n{hash}  app.exe\n"),
        format!("{hash}  app.exe extra\n"),
        "x  app.exe\n".into(),
        "".into(),
    ] {
        assert!(update::parse_checksums(bad.as_bytes()).is_err());
    }
}
#[test]
fn incomplete_changed_and_symlinked_packages_cannot_be_verified() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("package");
    let data = b"verified release bytes";
    std::fs::write(&path, data).unwrap();
    let hash = format!("{:x}", Sha256::digest(data));
    update::verify_file(&path, &hash).unwrap();
    std::fs::write(&path, b"partial").unwrap();
    assert!(update::verify_file(&path, &hash).is_err());
    #[cfg(unix)]
    {
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(update::verify_file(&link, &hash).is_err());
    }
}
