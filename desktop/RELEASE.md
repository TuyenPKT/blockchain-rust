# PKTScan Desktop — Build & Release Guide (v20.9)

## Yêu cầu

| Tool          | Version   | Cài đặt |
|---------------|-----------|---------|
| Rust          | stable    | `rustup update stable` |
| Node.js       | ≥ 20      | `nvm install 20` |
| Tauri CLI     | v2        | bundled via `@tauri-apps/cli` |

### macOS thêm:
```bash
xcode-select --install
```

### Linux (Ubuntu/Debian) thêm:
```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev patchelf libssl-dev pkg-config
```

### Windows:
- Visual Studio Build Tools (C++ workload)
- WebView2 Runtime (thường đã có sẵn trên Windows 11)

---

## Build thủ công

```bash
cd desktop

# 1. Install frontend deps
npm install

# 2. Build tất cả targets (native platform)
npm run tauri build

# Hoặc build target cụ thể:
npm run tauri build -- --target x86_64-apple-darwin   # macOS Intel
npm run tauri build -- --target aarch64-apple-darwin  # macOS Apple Silicon
npm run tauri build -- --target universal-apple-darwin # macOS Universal
npm run tauri build -- --target x86_64-pc-windows-msvc # Windows
npm run tauri build -- --target x86_64-unknown-linux-gnu # Linux
```

### Output files:

| Platform | Path | File |
|----------|------|------|
| macOS    | `desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/` | `PKTScan_*.dmg` |
| Windows  | `desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/` | `PKTScan_*.msi` |
| Linux    | `desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/` | `pktscan_*.AppImage` |
| Linux    | `desktop/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/deb/` | `pktscan_*.deb` |

---

## Release qua GitHub Actions (CI/CD)

Workflow file: `.github/workflows/release.yml`

**Trigger:** push tag dạng `v*.*.*`

```bash
# Bump version trong tauri.conf.json trước, commit, rồi:
git tag v0.8.0
git push origin v0.8.0
```

GitHub Actions sẽ tự động:
1. Build trên 3 runner: `macos-latest`, `windows-latest`, `ubuntu-22.04`
2. Tạo GitHub Release draft với tất cả artifacts đính kèm

### Secrets cần thiết (Settings → Secrets → Actions):

| Secret | Mô tả | Bắt buộc |
|--------|-------|----------|
| `APPLE_CERTIFICATE` | Base64 của .p12 developer certificate | macOS signing |
| `APPLE_CERTIFICATE_PASSWORD` | Password của .p12 | macOS signing |
| `APPLE_SIGNING_IDENTITY` | "Developer ID Application: ..." | macOS signing |
| `APPLE_ID` | Apple developer email | macOS notarization |
| `APPLE_PASSWORD` | App-specific password | macOS notarization |
| `APPLE_TEAM_ID` | Apple Team ID | macOS notarization |

> Nếu không có Apple certificate, build vẫn thành công nhưng macOS .dmg sẽ không được signed.
> Người dùng cần `xattr -cr /Applications/PKTScan.app` để mở lần đầu.

---

## Build check (PR / push to main)

Workflow file: `.github/workflows/build-check.yml`

Chạy tự động trên mỗi PR và push vào `main`:
- TypeScript strict `tsc --noEmit`
- `cargo build -p pktscan-desktop`
- `cargo test -p pktscan-desktop`
- `cargo test --workspace --exclude pktscan-desktop`

---

## Version bump checklist

Trước khi release:
- [ ] Update `version` trong `desktop/src-tauri/tauri.conf.json`
- [ ] Update version badge trong `desktop/src/components/Nav.tsx`
- [ ] Update CHANGELOG.md với entries cho version mới
- [ ] Update CONTEXT.md + CLAUDE.md
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `cargo build -p pktscan-desktop` — 0 warnings
- [ ] `git tag v<version> && git push origin v<version>`

---

## Dev mode

```bash
cd desktop
npm run tauri dev
```

Mở cửa sổ Tauri với hot-reload React. Rust backend rebuild khi sửa `src-tauri/src/`.
