<p align="center">
  <img src="./src-tauri/icons/icon.png" width="112" alt="PixLab Desktop icon" />
</p>

<h1 align="center">PixLab Desktop</h1>

<p align="center">
  A desktop studio for pixel art, spritesheets, animation previews, and screen pets.
</p>

<p align="center">
  <a href="./LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-f0b35a.svg" /></a>
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2.x-24c8db.svg" />
  <img alt="Windows" src="https://img.shields.io/badge/Windows-ready-2563eb.svg" />
  <img alt="macOS" src="https://img.shields.io/badge/macOS-ready-111111.svg" />
</p>

<p align="center">
  <a href="https://github.com/DAT1305/pixlab/releases/latest">
    <img alt="Download latest release" src="https://img.shields.io/badge/Download-latest%20release-111111?style=for-the-badge" />
  </a>
</p>

<p align="center">
  <a href="./README.vi.md">Tiếng Việt</a> ·
  <a href="./README.zh.md">中文</a>
</p>

## What It Does

PixLab Desktop gives creators one focused app for preparing pixel assets and lightweight animated companions. It works well for game prototypes, stickers, social posts, and animation tests.

## Features

- Convert images into crisp pixel-art output.
- Clean sprites and remove simple backgrounds.
- Slice spritesheets, align frames, preview motion, and export GIFs.
- Generate animation sheets from text or reference images.
- Create animated pets, preview their actions, and attach them to the desktop.
- Keep a pet history and a curated pet library.
- Import and export pet files for sharing.
- Check for app updates from GitHub Releases.

## Download

Prebuilt installers are published on the [Releases page](https://github.com/DAT1305/pixlab/releases/latest).

Windows builds include both 64-bit and 32-bit installers when available.

## Run From Source

Requirements:

- Node.js 20+
- Rust stable
- npm
- Windows Build Tools or the equivalent platform SDK

```bash
npm ci
npm run dev
```

## Build

```bash
npm run build:windows      # Windows 64-bit installer
npm run build:windows:x86  # Windows 32-bit installer
npm run build:mac          # macOS app and DMG, run on macOS
```

## macOS Note

The public macOS build may not be notarized yet. If Gatekeeper blocks the app after copying it to Applications, run:

```bash
xattr -dr com.apple.quarantine "/Applications/PixLab Desktop.app"
```

## License

MIT
