# GT7 Telemetry App

Tauri + React desktop app for Gran Turismo 7 telemetry capture, live dashboard, and lap analysis.

## Credits

- Telemetry packet mapping and inspiration: https://github.com/hschaefer123/racecap?tab=readme-ov-file
- Dashboard analysis references: https://github.com/snipem/gt7dashboard/blob/main/README.assets

## Requirements

- Node.js 18+
- Rust toolchain (stable)
- Tauri prerequisites for your OS

## Setup

```bash
npm install
```

## Run (dev)

```bash
npm run tauri dev
```

## Use

1) Enter your PS5 IP address
2) Start the listener
3) Initialize the database
4) Drive in GT7 to see live metrics and record laps

## Notes

- UDP ports 33739/33740 are used for telemetry
- Database path is shown in the app under Database
