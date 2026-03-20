# TokenMonitor – Dev Notes

## Dev server

```bash
npm run tauri dev
```

## Building a release DMG

The build requires macOS code signing and notarization. All signing files live in
`signing/` (gitignored – never commit them).

### Prerequisites

| What | Where |
|------|-------|
| Developer ID Application cert | Installed in Keychain (`Developer ID Application: Zimo Luo (DY9X92M8C7)`) |
| Developer ID CA intermediate | Installed in Keychain (imported from Apple's CA page) |
| App Store Connect API key | `signing/AuthKey_55WD7ZCG9H.p8` |
| API Key ID | `55WD7ZCG9H` |
| Issuer ID | `0879863a-8541-46ac-8b53-7e3f2dc3f821` |
| Team ID | `DY9X92M8C7` |

Verify the keychain is ready before building:
```bash
security find-identity -v -p codesigning
# Should show: "Developer ID Application: Zimo Luo (DY9X92M8C7)"
```

If the identity is missing, the intermediate cert may need to be re-imported:
```bash
curl -s "https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer" -o /tmp/DeveloperIDG2CA.cer
security import /tmp/DeveloperIDG2CA.cer -k ~/Library/Keychains/login.keychain-db
```

### Build command

```bash
APPLE_SIGNING_IDENTITY="Developer ID Application: Zimo Luo (DY9X92M8C7)" \
APPLE_TEAM_ID="DY9X92M8C7" \
APPLE_API_KEY="55WD7ZCG9H" \
APPLE_API_ISSUER="0879863a-8541-46ac-8b53-7e3f2dc3f821" \
APPLE_API_KEY_PATH="$(pwd)/signing/AuthKey_55WD7ZCG9H.p8" \
npm run tauri build -- --bundles dmg
```

Output: `src-tauri/target/release/bundle/dmg/TokenMonitor_<version>_aarch64.dmg`

## Versioning and releases

After committing changes, assess whether a version bump is warranted and, if so, apply it automatically:

- **Patch** (0.0.x): bug fixes, config tweaks, build/CI changes
- **Minor** (0.x.0): new features, new UI elements, new settings
- **Major** (x.0.0): breaking changes, major redesigns, data format changes

If the change is significant enough to bump:

1. Update version in all three files: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
2. Run `cargo generate-lockfile` in `src-tauri/` to sync `Cargo.lock`
3. Commit: `chore(release): bump version to X.Y.Z`
4. Tag: `git tag -a vX.Y.Z -m "vX.Y.Z"`
5. Push: `git push origin main --follow-tags`

The tag push triggers the GitHub Actions release workflow (`.github/workflows/release.yml`) which builds, signs, notarizes, and publishes the DMG.

## signing/ directory contents

```
signing/
  AuthKey_55WD7ZCG9H.p8     # App Store Connect API key (notarization)
  developerID_application.cer # Developer ID cert backup (already in Keychain)
```
