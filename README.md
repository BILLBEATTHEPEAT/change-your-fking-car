# Change Your Fking Car

GT7 telemetry workstation with a live dashboard, lap analysis, and session storage.

## Credits

- Telemetry packet mapping and inspiration: https://github.com/hschaefer123/racecap?tab=readme-ov-file
- Dashboard analysis references: https://github.com/snipem/gt7dashboard/blob/main/README.assets

## What it does

- Collects Gran Turismo 7 telemetry over UDP
- Shows a live cockpit dashboard (speed, RPM, gear, lap, fuel, temps, tire temps, flags)
- Saves per-lap telemetry samples to a local SQLite database for review

## Requirements

- Node.js 18+
- Rust toolchain (stable)
- Tauri prerequisites for your OS
- A PS5 running GT7 on the same network

## Getting started

1) Install dependencies

```bash
cd gt7-telemetry-app
npm install
```

2) Run the app

```bash
npm run tauri dev
```

3) Connect to GT7

- Enter your PS5 IP in the app
- Click "Start Listener"
- Click "Initialize DB" to create the session database
- Drive in GT7 and watch the Live Dashboard update

## Data storage

- Telemetry is stored in a local SQLite DB
- Path is shown in the app under "Database"
- Each lap has a sample stream for analysis

## Notes

- UDP ports 33739/33740 are used for telemetry
- Ensure GT7 telemetry broadcast is enabled and the app has network access

## Repo layout

- `gt7-telemetry-app/` Tauri + React desktop app
- `docs/` Design and planning notes
