# Horizon Next

A Minecraft launcher written in Rust with a redesigned dark UI (egui/eframe).

## Features
- Sidebar navigation: Play, Instances, Settings, About
- Hero banner + one-click Play with live progress
- Multiple instances (version + RAM each), release/snapshot picker
- Downloads client, libraries, natives and assets straight from Mojang (8 parallel asset downloads)
- Offline-mode username (no Microsoft login yet)

## Build & run
Requires Rust (https://rustup.rs) and Java 17/21 on your PATH.

```
cargo run --release
```

On Linux you may need: `libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev`.

Data (config, versions, libraries, assets, instances) is stored in your OS data directory
(shown in Settings).

## Layout
- `src/main.rs` – UI
- `src/theme.rs` – colors and egui style
- `src/launcher.rs` – version manifest, downloads, launch command
- `src/config.rs` – persisted settings/instances

## Microsoft sign-in
Uses the device-code flow (Microsoft → Xbox Live → XSTS → Minecraft services).
You need your own Azure app registration:
1. portal.azure.com → App registrations → New registration
   - Supported account types: "Personal Microsoft accounts only"
   - Authentication → enable "Allow public client flows"
2. Copy the Application (client) ID into Settings → Azure Client ID.
3. Mojang requires new apps to be approved before the Minecraft API accepts them
   (request via the form on https://aka.ms/AppRegInfo). Until approved, sign-in fails at the Minecraft step.

Your refresh token is stored in `account.json` in the data folder (plain text).

## Downloadable builds
Push this folder to a GitHub repo. The workflow in `.github/workflows/release.yml`
builds Windows/Linux/macOS executables on every run (Actions → build → Artifacts),
and publishes them as Release assets when you push a tag such as `v0.1.0`.
