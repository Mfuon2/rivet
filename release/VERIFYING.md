# Verify a Rivet release

Release packages are accompanied by `SHA256SUMS` and `SHA256SUMS.minisig`. A checksum is not a signature. Do not treat an unsigned checksum file as publisher authentication.

1. Install [Minisign](https://jedisct1.github.io/minisign/).
2. Obtain Rivet's public release key from the established official repository and verify its fingerprint/public-key value through a separate trusted channel. A key supplied only beside an untrusted download is not a trust anchor.
3. Download the release packages, checksum file and signature into one directory.
4. Authenticate the checksum file before trusting its contents:

```sh
minisign -Vm SHA256SUMS -p /path/to/trusted/minisign.pub
```

5. Verify the package hashes. Linux: `sha256sum -c SHA256SUMS`. macOS: `shasum -a 256 -c SHA256SUMS`. For a cross-platform verifier, use the trusted repository script:

```sh
python3 scripts/release.py verify /path/to/downloads --pubkey /path/to/trusted/minisign.pub
```

The script checks the signature first, rejects unsafe/duplicate manifest entries and verifies every listed package. Download all packages listed in this manifest when using its strict directory verifier. The signature covers the release manifest, not operating-system publisher identity. Windows Authenticode and Apple Developer ID/notarization are separate and are not claimed by this workflow.

Rivet's public release key is published at `release/minisign.pub` in the official `Mfuon2/rivet` repository. Its initial key ID is `FACB200CA8550759`. Verify the full public-key value against the established source repository; the short key ID alone is not a trust anchor. Never accept a test key as Rivet's release identity.
