# GT7 Telemetry Mac App Plan

This document captures the agreed implementation plan for a macOS
sim-racing telemetry and analysis app that matches the feature depth
of `gt7dashboard`, implemented with Tauri + React and a Rust backend.

## Goals

- Match the analysis depth of `gt7dashboard` while running as a native
  macOS desktop app via Tauri.
- Use a Rust backend for UDP telemetry ingest, analysis, and SQLite
  persistence.
- Use a React frontend with rich, interactive charts (ECharts).
- Support lap/session save and load via JSON snapshots (gt7dashboard-style).

## Tech Decisions

- Frontend: React
- Charts: ECharts (via `echarts-for-react`)
- Backend: Rust (Tauri commands + background workers)
- Storage: SQLite (primary) + JSON import/export
- Target OS: macOS first (Tauri bundle), with room for cross-platform later

## Feature Scope (v1 - parity with gt7dashboard depth)

- Live telemetry dashboard (speed, throttle, brake, coast, RPM, gear,
  yaw rate, boost, tire vs car speed, etc.).
- Lap comparison and reference lap selection.
- Time delta graph (last lap vs reference).
- Speed/Distance graph (last, reference, median lap).
- Speed variance visualization for best laps.
- Race line map with peaks/valleys and throttle/brake/coast zones.
- Lap table with detailed metrics and replay flags.
- Fuel map helper and tuning metrics (max speed, min body height).
- Save current laps and load saved sessions.
- Optional replay capture and analysis.

## Milestones

### M1 - Data Model + UDP Parser

- Define core types:
  - Session
  - Lap
  - Sample
  - Car
  - Track
  - TelemetryPacket
- Implement UDP listener for GT7 packets:
  - Bind to ports 33740/33739.
  - Support broadcast discovery (default) and manual PS5 IP override.
- Parse packets into normalized samples with timestamps and distance.
- Detect lap boundaries and replay flags.

### M2 - SQLite Persistence

- Create schema:
  - sessions(id, started_at, track_id, car_id, notes, is_replay)
  - laps(id, session_id, lap_index, lap_time_ms, is_valid, is_replay)
  - samples(id, lap_id, ts_ms, dist_m, speed_kmh, throttle, brake, rpm,
            gear, yaw_rate, x, y, z, fuel, body_height, flags, ...)
  - cars(id, name, source)
  - tracks(id, name, source)
- Add indexes:
  - laps.session_id
  - samples.lap_id
  - samples.ts_ms
  - samples.dist_m

### M3 - Analysis Engine

- Lap alignment and resampling (distance-normalized comparisons).
- Derived metrics:
  - Time delta (last vs reference)
  - Median lap
  - Speed variance across best laps
  - Peaks/valleys detection
  - Throttle/brake/coast zones
  - Fuel map estimation
  - Tuning metrics (max speed, min body height)
- Produce chart-friendly payloads (downsampled series and summaries).

### M4 - React UI (Tauri)

- Shell and app state:
  - Global session/lap selection
  - Live connection state + recording controls
- Views:
  - Live Dashboard
  - Get Faster (lap compare + deltas + metrics table)
  - Race Line (map + overlays + markers)
  - Session Library (saved sessions, import/export)
- Charts:
  - Time delta graph
  - Speed/Distance
  - Throttle/Brake/Coast
  - RPM/Gear/Yaw
  - Race line map with peak/valley markers

### M5 - Save/Load (gt7dashboard-style)

- Export JSON session file (laps + samples + metadata).
- Import JSON into SQLite with ID remapping.
- UI flow:
  - "Save Laps" button
  - "Load Laps" dropdown or picker

### M6 - Performance + UX

- Stream throttling and decimation for charts.
- Move analysis to background worker thread.
- Cache computed lap comparisons.
- Clear connection/recording status indicators.

### M7 - macOS Packaging

- Tauri bundle settings (icons, entitlements, permissions).
- Code signing and notarization checklist.
- Optional auto-update strategy.

## Step-by-Step Task Checklist

### 1) Repository + Project Scaffold

- Create a new Tauri + React project (Vite template).
- Verify `src-tauri` is generated and Rust toolchain is set.
- Add a minimal app shell with a single route.

### 2) Rust Backend Skeleton

- Add Tauri commands module with placeholder APIs:
  - `start_listener`
  - `stop_listener`
  - `get_live_snapshot`
  - `list_sessions`
  - `load_session`
  - `export_session_json`
  - `import_session_json`
- Implement logging and error handling conventions.

### 3) UDP Telemetry Ingest

- Implement UDP socket binding for GT7 ports (33740/33739).
- Add broadcast discovery, fallback to manual PS5 IP.
- Parse GT7 packet into typed struct and normalized sample.
- Confirm lap boundary detection and replay flags.

### 4) SQLite Persistence

- Add migration for schema (sessions/laps/samples/cars/tracks).
- Write inserts for sessions, laps, samples.
- Index high-traffic columns.

### 5) Analysis Core

- Implement lap alignment and distance normalization.
- Build derived metrics (time delta, median lap, variance, peaks/valleys).
- Create chart-ready payload generator.

### 6) React UI Foundation

- Layout shell (sidebar + main content).
- Build global state for sessions/laps/connection.
- Integrate Tauri command calls.

### 7) Views + Charts

- Live Dashboard with real-time charts.
- Get Faster view with lap compare + tables.
- Race Line map with overlays and markers.
- Session Library with import/export.

### 8) Save/Load Workflow

- Implement JSON export and import.
- UI buttons and file picker integration.

### 9) Performance + UX Polish

- Stream decimation for charts.
- Background analysis worker.
- Visual connection/recording indicators.

### 10) macOS Packaging

- Configure entitlements and permissions.
- Build and notarize app bundle.
- Prepare DMG and release checklist.

## Open Items (Non-blocking)

- Decide on React state management (recommend `zustand`).
- Choose Rust SQLite crate (`rusqlite` recommended initially).
- Decide JSON export format (flat vs nested per lap).

## References

- https://github.com/snipem/gt7dashboard
- https://github.com/hschaefer123/racecap
- https://github.com/AleBles/gt-telemetry
