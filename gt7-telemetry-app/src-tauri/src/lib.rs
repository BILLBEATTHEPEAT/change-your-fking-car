use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

#[derive(Default)]
struct SharedState {
    listener_running: Mutex<bool>,
    last_packet_at: Mutex<Option<i64>>,
    packet_count: Mutex<u64>,
    last_packet_meta: Mutex<Option<PacketMeta>>,
    last_sample: Mutex<Option<TelemetrySampleSummary>>,
    last_live_payload: Mutex<Option<LivePayload>>,
    last_heartbeat_at: Mutex<Option<i64>>,
    last_listener_error: Mutex<Option<String>>,
    bound_ports: Mutex<Vec<u16>>,
    current_session_id: Mutex<Option<i64>>,
}

struct AppState {
    shared: Arc<SharedState>,
    stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    db_path: Mutex<Option<PathBuf>>,
    target_ip: Mutex<Option<String>>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct PacketMeta {
    magic: Option<u32>,
    packet_id: Option<u32>,
    payload_len: usize,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct TelemetrySampleSummary {
    packet_id: i32,
    lap_count: i16,
    car_code: i32,
    speed_kmh: f32,
    engine_rpm: f32,
    throttle: u8,
    brake: u8,
    gear: u8,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct LivePayload {
    packet_id: i32,
    lap_count: i16,
    laps_in_race: i16,
    speed_kmh: f32,
    engine_rpm: f32,
    gear: u8,
    suggested_gear: u8,
    throttle: u8,
    brake: u8,
    fuel_level: f32,
    fuel_capacity: f32,
    fuel_pct: f32,
    tire_temp_fl: f32,
    tire_temp_fr: f32,
    tire_temp_rl: f32,
    tire_temp_rr: f32,
    water_temp: f32,
    oil_temp: f32,
    oil_pressure: f32,
    current_lap_time_ms: i32,
    last_lap_time_ms: i32,
    best_lap_time_ms: i32,
    pre_race_pos: i16,
    num_cars_pre_race: i16,
    asm_active: bool,
    tcs_active: bool,
    rev_limiter_active: bool,
    car_on_track: bool,
    paused: bool,
    loading_or_processing: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    version: String,
    listener_running: bool,
    last_packet_at: Option<i64>,
    packet_count: u64,
    last_packet_meta: Option<PacketMeta>,
    db_path: Option<String>,
    last_sample: Option<TelemetrySampleSummary>,
    target_ip: Option<String>,
    last_heartbeat_at: Option<i64>,
    last_listener_error: Option<String>,
    bound_ports: Vec<u16>,
    current_session_id: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DatabaseInfo {
    path: String,
    exists: bool,
    size_bytes: Option<u64>,
    sessions: Option<i64>,
    laps: Option<i64>,
    samples: Option<i64>,
    last_sample_ts: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DbInitResult {
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentSample {
    ts_ms: i64,
    speed_kmh: f64,
    throttle: f64,
    brake: f64,
    rpm: f64,
    gear: i32,
    lap_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LapSummary {
    id: i64,
    lap_index: i32,
    lap_time_ms: Option<i64>,
    is_valid: bool,
    is_replay: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummary {
    id: i64,
    started_at: i64,
    best_lap_ms: Option<i64>,
    lap_count: i64,
    duration_ms: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionPreferences {
    reference_lap_id: Option<i64>,
    compare_lap_id: Option<i64>,
    smooth_lines: Option<i64>,
    show_legends: Option<i64>,
    race_line_color_mode: Option<String>,
    show_peaks: Option<i64>,
    peak_threshold: Option<i64>,
    peak_spacing: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LapSamplePoint {
    idx: i64,
    ts_ms: i64,
    speed_kmh: f64,
    throttle: f64,
    brake: f64,
    rpm: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportSample {
    ts_ms: i64,
    speed_kmh: f64,
    throttle: f64,
    brake: f64,
    rpm: f64,
    gear: i32,
    x: f64,
    z: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportLap {
    id: i64,
    lap_index: i32,
    lap_time_ms: Option<i64>,
    samples: Vec<ExportSample>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionExport {
    session_id: i64,
    preferences: SessionPreferences,
    laps: Vec<ExportLap>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackPoint {
    x: f64,
    z: f64,
    throttle: f64,
    brake: f64,
}

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

#[tauri::command]
fn get_app_status(state: State<AppState>) -> AppStatus {
    let listener_running = *state.shared.listener_running.lock().unwrap();
    let last_packet_at = *state.shared.last_packet_at.lock().unwrap();
    let packet_count = *state.shared.packet_count.lock().unwrap();
    let last_packet_meta = *state.shared.last_packet_meta.lock().unwrap();
    let last_sample = *state.shared.last_sample.lock().unwrap();
    let last_heartbeat_at = *state.shared.last_heartbeat_at.lock().unwrap();
    let last_listener_error = state.shared.last_listener_error.lock().unwrap().clone();
    let bound_ports = state.shared.bound_ports.lock().unwrap().clone();
    let current_session_id = *state.shared.current_session_id.lock().unwrap();
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .as_ref()
        .map(|path| path.to_string_lossy().to_string());
    let target_ip = state.target_ip.lock().unwrap().clone();

    AppStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        listener_running,
        last_packet_at,
        packet_count,
        last_packet_meta,
        db_path,
        last_sample,
        target_ip,
        last_heartbeat_at,
        last_listener_error,
        bound_ports,
        current_session_id,
    }
}

#[tauri::command]
fn get_live_payload(state: State<AppState>) -> Option<LivePayload> {
    let payload = *state.shared.last_live_payload.lock().unwrap();
    payload
}

#[tauri::command]
fn get_database_info(state: State<AppState>) -> Result<DatabaseInfo, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    let path_str = db_path.to_string_lossy().to_string();
    let exists = db_path.exists();
    let size_bytes = if exists {
        std::fs::metadata(&db_path).ok().map(|m| m.len())
    } else {
        None
    };

    if !exists {
        return Ok(DatabaseInfo {
            path: path_str,
            exists,
            size_bytes,
            sessions: None,
            laps: None,
            samples: None,
            last_sample_ts: None,
        });
    }

    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;
    let laps: i64 = conn
        .query_row("SELECT COUNT(*) FROM laps", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;
    let samples: i64 = conn
        .query_row("SELECT COUNT(*) FROM samples", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;
    let last_sample_ts: Option<i64> = conn
        .query_row("SELECT MAX(ts_ms) FROM samples", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;

    Ok(DatabaseInfo {
        path: path_str,
        exists,
        size_bytes,
        sessions: Some(sessions),
        laps: Some(laps),
        samples: Some(samples),
        last_sample_ts,
    })
}

#[tauri::command]
fn vacuum_database(state: State<AppState>) -> Result<(), String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    conn.execute_batch("VACUUM;")
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
fn reset_database(state: State<AppState>) -> Result<(), String> {
    let running = *state.shared.listener_running.lock().unwrap();
    if running {
        return Err("Stop the listener before resetting the database".to_string());
    }
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    if db_path.exists() {
        std::fs::remove_file(&db_path).map_err(|err| err.to_string())?;
    }
    *state.shared.current_session_id.lock().unwrap() = None;
    *state.shared.last_sample.lock().unwrap() = None;
    *state.shared.last_live_payload.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
fn delete_lap(state: State<AppState>, lap_id: i64) -> Result<(), String> {
    let running = *state.shared.listener_running.lock().unwrap();
    if running {
        return Err("Stop the listener before deleting data".to_string());
    }
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let mut conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    tx.execute("DELETE FROM samples WHERE lap_id = ?1", params![lap_id])
        .map_err(|err| err.to_string())?;
    tx.execute("DELETE FROM laps WHERE id = ?1", params![lap_id])
        .map_err(|err| err.to_string())?;
    tx.commit().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
fn delete_session(state: State<AppState>, session_id: i64) -> Result<(), String> {
    let running = *state.shared.listener_running.lock().unwrap();
    if running {
        return Err("Stop the listener before deleting data".to_string());
    }
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let mut conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let tx = conn.transaction().map_err(|err| err.to_string())?;
    tx.execute(
        "DELETE FROM samples WHERE lap_id IN (SELECT id FROM laps WHERE session_id = ?1)",
        params![session_id],
    )
    .map_err(|err| err.to_string())?;
    tx.execute(
        "DELETE FROM laps WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|err| err.to_string())?;
    tx.execute(
        "DELETE FROM session_preferences WHERE session_id = ?1",
        params![session_id],
    )
    .map_err(|err| err.to_string())?;
    tx.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
        .map_err(|err| err.to_string())?;
    tx.commit().map_err(|err| err.to_string())?;

    let mut current = state.shared.current_session_id.lock().unwrap();
    if *current == Some(session_id) {
        *current = None;
    }
    Ok(())
}

#[tauri::command]
fn set_target_ip(state: State<AppState>, ip: String) -> AppStatus {
    let trimmed = ip.trim().to_string();
    if trimmed.is_empty() {
        *state.target_ip.lock().unwrap() = None;
    } else {
        *state.target_ip.lock().unwrap() = Some(trimmed);
    }
    get_app_status(state)
}

#[tauri::command]
fn start_listener(state: State<AppState>) -> AppStatus {
    let mut running_guard = state.shared.listener_running.lock().unwrap();
    if *running_guard {
        drop(running_guard);
        return get_app_status(state);
    }
    *running_guard = true;
    drop(running_guard);

    let (stop_tx, stop_rx) = mpsc::channel();
    *state.stop_tx.lock().unwrap() = Some(stop_tx);

    let shared = Arc::clone(&state.shared);
    let target_ip = state.target_ip.lock().unwrap().clone();
    let db_path = state.db_path.lock().unwrap().clone();
    std::thread::spawn(move || {
        let mut sockets = Vec::new();
        let mut bound = Vec::new();
        let mut last_error = None;
        let send_socket = UdpSocket::bind("0.0.0.0:0").ok();
        if let Some(socket) = send_socket.as_ref() {
            let _ = socket.set_broadcast(true);
        }

        let mut conn = match db_path.as_ref() {
            Some(path) => {
                if let Err(err) = init_database_at_path(path) {
                    *shared.last_listener_error.lock().unwrap() = Some(err.to_string());
                    None
                } else {
                    Connection::open(path).ok()
                }
            }
            None => None,
        };

        let mut session_id: Option<i64> = None;
        let mut current_lap_id: Option<i64> = None;
        let mut current_lap_index: Option<i16> = None;
        let mut current_lap_dist_m: f64 = 0.0;
        let mut last_sample_ts_ms: Option<i64> = None;
        let mut lap_start_time_of_day_ms: Option<i32> = None;

        for port in [33740, 33739] {
            if bound.contains(&port) {
                continue;
            }
            let bind_addr = format!("0.0.0.0:{port}");
            match UdpSocket::bind(bind_addr) {
                Ok(socket) => {
                    let _ = socket.set_broadcast(true);
                    let _ = socket.set_nonblocking(true);
                    sockets.push(socket);
                    bound.push(port);
                }
                Err(err) => {
                    last_error = Some(format!("Failed to bind UDP port {port}: {err}"));
                }
            }
        }

        if sockets.is_empty() {
            *shared.listener_running.lock().unwrap() = false;
            *shared.last_listener_error.lock().unwrap() =
                Some(last_error.unwrap_or_else(|| "Failed to bind any UDP ports".to_string()));
            return;
        }

        *shared.bound_ports.lock().unwrap() = bound;
        *shared.last_listener_error.lock().unwrap() = last_error;

        let mut buf = [0u8; 4096];
        let mut last_heartbeat = std::time::Instant::now() - Duration::from_secs(60);

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            if last_heartbeat.elapsed().as_secs() >= 10 {
                let heartbeat_result = if let Some(ip) = target_ip.as_ref() {
                    match send_socket.as_ref() {
                        Some(sock) => send_heartbeat(sock, ip),
                        None => Err("Heartbeat socket unavailable".to_string()),
                    }
                } else {
                    match send_socket.as_ref() {
                        Some(sock) => send_broadcast_heartbeat(sock),
                        None => Err("Heartbeat socket unavailable".to_string()),
                    }
                };
                if let Err(err) = heartbeat_result {
                    *shared.last_listener_error.lock().unwrap() = Some(err);
                } else {
                    let mut last_heartbeat_at = shared.last_heartbeat_at.lock().unwrap();
                    *last_heartbeat_at = Some(now_millis());
                    *shared.last_listener_error.lock().unwrap() = None;
                }
                last_heartbeat = std::time::Instant::now();
            }

            for socket in &sockets {
                match socket.recv_from(&mut buf) {
                    Ok((size, _addr)) => {
                        let mut last_packet_at = shared.last_packet_at.lock().unwrap();
                        *last_packet_at = Some(now_millis());
                        drop(last_packet_at);

                        let mut packet_count = shared.packet_count.lock().unwrap();
                        *packet_count = packet_count.saturating_add(1);
                        drop(packet_count);

                        let mut payload = buf[..size].to_vec();
                        if decrypt_gt7_packet(&mut payload).is_err() {
                            continue;
                        }

                        let meta = parse_packet_meta(&payload, size);
                        let mut last_packet_meta = shared.last_packet_meta.lock().unwrap();
                        *last_packet_meta = Some(meta);
                        drop(last_packet_meta);

                        if let Ok(sample) = parse_telemetry_sample(&payload, size) {
                            let now_ts_ms = now_millis();
                            let dt_seconds = last_sample_ts_ms
                                .map(|prev| (now_ts_ms - prev).max(0) as f64 / 1000.0)
                                .unwrap_or(0.0);
                            current_lap_dist_m += sample.meters_per_second as f64 * dt_seconds;
                            last_sample_ts_ms = Some(now_ts_ms);

                            if current_lap_index != Some(sample.lap_count) {
                                current_lap_dist_m = 0.0;
                                last_sample_ts_ms = Some(now_ts_ms);
                                lap_start_time_of_day_ms = Some(sample.time_of_day_ms);
                            }

                            if sample.lap_count == 0 {
                                lap_start_time_of_day_ms = Some(sample.time_of_day_ms);
                            }

                            let current_lap_time_ms = lap_start_time_of_day_ms
                                .map(|start| sample.time_of_day_ms.saturating_sub(start))
                                .unwrap_or(0);

                            let asm_active = (sample.flags & 1024) != 0;
                            let tcs_active = (sample.flags & 2048) != 0;
                            let rev_limiter_active = (sample.flags & 32) != 0;
                            let car_on_track = (sample.flags & 1) != 0;
                            let paused = (sample.flags & 2) != 0;
                            let loading_or_processing = (sample.flags & 4) != 0;

                            if let Some(conn) = conn.as_mut() {
                                if session_id.is_none() {
                                    if let Ok(id) = create_session(conn) {
                                        session_id = Some(id);
                                        *shared.current_session_id.lock().unwrap() = Some(id);
                                    }
                                }

                                if let Some(session_id) = session_id {
                                    if current_lap_index != Some(sample.lap_count) {
                                        if let Some(prev_lap_id) = current_lap_id {
                                            if sample.last_lap_time_ms > 0 {
                                                let _ = update_lap_time(
                                                    conn,
                                                    prev_lap_id,
                                                    sample.last_lap_time_ms as i64,
                                                );
                                            }
                                        }

                                        if let Ok(lap_id) =
                                            create_lap(conn, session_id, sample.lap_count as i32)
                                        {
                                            current_lap_id = Some(lap_id);
                                            current_lap_index = Some(sample.lap_count);
                                        }
                                    }

                                    if let Some(lap_id) = current_lap_id {
                                        let _ = insert_sample(
                                            conn,
                                            lap_id,
                                            &sample,
                                            now_ts_ms,
                                            current_lap_dist_m,
                                            current_lap_time_ms,
                                            asm_active,
                                            tcs_active,
                                            rev_limiter_active,
                                            car_on_track,
                                            paused,
                                            loading_or_processing,
                                        );
                                    }
                                }
                            }

                            let summary = TelemetrySampleSummary {
                                packet_id: sample.packet_id,
                                lap_count: sample.lap_count,
                                car_code: sample.car_code,
                                speed_kmh: sample.meters_per_second * 3.6,
                                engine_rpm: sample.engine_rpm,
                                throttle: sample.throttle,
                                brake: sample.brake,
                                gear: sample.current_gear,
                            };
                            let mut last_sample = shared.last_sample.lock().unwrap();
                            *last_sample = Some(summary);

                            let fuel_pct = if sample.gas_capacity > 0.0 {
                                (sample.gas_level / sample.gas_capacity) * 100.0
                            } else {
                                0.0
                            };
                            let live_payload = LivePayload {
                                packet_id: sample.packet_id,
                                lap_count: sample.lap_count,
                                laps_in_race: sample.laps_in_race,
                                speed_kmh: sample.meters_per_second * 3.6,
                                engine_rpm: sample.engine_rpm,
                                gear: sample.current_gear,
                                suggested_gear: sample.suggested_gear,
                                throttle: sample.throttle,
                                brake: sample.brake,
                                fuel_level: sample.gas_level,
                                fuel_capacity: sample.gas_capacity,
                                fuel_pct,
                                tire_temp_fl: sample.tire_surface_temp[0],
                                tire_temp_fr: sample.tire_surface_temp[1],
                                tire_temp_rl: sample.tire_surface_temp[2],
                                tire_temp_rr: sample.tire_surface_temp[3],
                                water_temp: sample.water_temperature,
                                oil_temp: sample.oil_temperature,
                                oil_pressure: sample.oil_pressure,
                                current_lap_time_ms,
                                last_lap_time_ms: sample.last_lap_time_ms,
                                best_lap_time_ms: sample.best_lap_time_ms,
                                pre_race_pos: sample.pre_race_pos,
                                num_cars_pre_race: sample.num_cars_pre_race,
                                asm_active,
                                tcs_active,
                                rev_limiter_active,
                                car_on_track,
                                paused,
                                loading_or_processing,
                            };
                            let mut last_live_payload = shared.last_live_payload.lock().unwrap();
                            *last_live_payload = Some(live_payload);
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(_err) => continue,
                }
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        *shared.listener_running.lock().unwrap() = false;
    });

    get_app_status(state)
}

#[tauri::command]
fn stop_listener(state: State<AppState>) -> AppStatus {
    if let Some(tx) = state.stop_tx.lock().unwrap().take() {
        let _ = tx.send(());
    }
    get_app_status(state)
}

#[tauri::command]
fn init_database(state: State<AppState>) -> Result<DbInitResult, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;

    init_database_at_path(&db_path).map_err(|err| err.to_string())?;

    Ok(DbInitResult {
        path: db_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn get_recent_samples(state: State<AppState>, limit: u32) -> Result<Vec<RecentSample>, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "
      SELECT ts_ms, speed_kmh, throttle, brake, rpm, gear, lap_id
      FROM samples
      ORDER BY id DESC
      LIMIT ?1
      ",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([limit], |row| {
            Ok(RecentSample {
                ts_ms: row.get(0)?,
                speed_kmh: row.get(1)?,
                throttle: row.get(2)?,
                brake: row.get(3)?,
                rpm: row.get(4)?,
                gear: row.get(5)?,
                lap_id: row.get(6)?,
            })
        })
        .map_err(|err| err.to_string())?;

    let mut samples = Vec::new();
    for item in rows {
        samples.push(item.map_err(|err| err.to_string())?);
    }
    Ok(samples)
}

#[tauri::command]
fn list_laps(state: State<AppState>, session_id: Option<i64>) -> Result<Vec<LapSummary>, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "
      SELECT id, lap_index, lap_time_ms, is_valid, is_replay
      FROM laps
      WHERE (?1 IS NULL OR session_id = ?1)
      ORDER BY id DESC
      ",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([session_id], |row| {
            Ok(LapSummary {
                id: row.get(0)?,
                lap_index: row.get::<_, i64>(1)? as i32,
                lap_time_ms: row.get(2)?,
                is_valid: row.get::<_, i64>(3)? != 0,
                is_replay: row.get::<_, i64>(4)? != 0,
            })
        })
        .map_err(|err| err.to_string())?;

    let mut laps = Vec::new();
    for item in rows {
        laps.push(item.map_err(|err| err.to_string())?);
    }
    Ok(laps)
}

#[tauri::command]
fn list_sessions(state: State<AppState>) -> Result<Vec<SessionSummary>, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "
      SELECT
        s.id,
        s.started_at,
        MIN(l.lap_time_ms) AS best_lap_ms,
        COUNT(l.id) AS lap_count,
        (MAX(sm.ts_ms) - MIN(sm.ts_ms)) AS duration_ms
      FROM sessions s
      LEFT JOIN laps l ON l.session_id = s.id
      LEFT JOIN samples sm ON sm.lap_id = l.id
      GROUP BY s.id
      ORDER BY s.started_at DESC
      ",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                started_at: row.get(1)?,
                best_lap_ms: row.get(2).ok(),
                lap_count: row.get::<_, i64>(3)?,
                duration_ms: row.get(4).ok(),
            })
        })
        .map_err(|err| err.to_string())?;

    let mut sessions = Vec::new();
    for item in rows {
        sessions.push(item.map_err(|err| err.to_string())?);
    }
    Ok(sessions)
}

#[tauri::command]
fn set_current_session(state: State<AppState>, session_id: i64) -> Result<(), String> {
    *state.shared.current_session_id.lock().unwrap() = Some(session_id);
    Ok(())
}

#[tauri::command]
fn get_session_preferences(state: State<AppState>) -> Result<SessionPreferences, String> {
    let session_id = *state.shared.current_session_id.lock().unwrap();
    let Some(session_id) = session_id else {
        return Ok(SessionPreferences {
            reference_lap_id: None,
            compare_lap_id: None,
            smooth_lines: None,
            show_legends: None,
            race_line_color_mode: None,
            show_peaks: None,
            peak_threshold: None,
            peak_spacing: None,
        });
    };

    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    read_session_preferences(&conn, session_id).map_err(|err| err.to_string())
}

#[tauri::command]
fn set_session_preferences(
    state: State<AppState>,
    reference_lap_id: Option<i64>,
    compare_lap_id: Option<i64>,
    smooth_lines: Option<bool>,
    show_legends: Option<bool>,
    race_line_color_mode: Option<String>,
    show_peaks: Option<bool>,
    peak_threshold: Option<i64>,
    peak_spacing: Option<i64>,
) -> Result<(), String> {
    let session_id = *state.shared.current_session_id.lock().unwrap();
    let Some(session_id) = session_id else {
        return Ok(());
    };

    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    conn.execute(
        "
      INSERT INTO session_preferences (
        session_id,
        reference_lap_id,
        compare_lap_id,
        smooth_lines,
        show_legends,
        race_line_color_mode,
        show_peaks,
        peak_threshold,
        peak_spacing
      )
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
      ON CONFLICT(session_id) DO UPDATE SET
        reference_lap_id = excluded.reference_lap_id,
        compare_lap_id = excluded.compare_lap_id,
        smooth_lines = excluded.smooth_lines,
        show_legends = excluded.show_legends,
        race_line_color_mode = excluded.race_line_color_mode,
        show_peaks = excluded.show_peaks,
        peak_threshold = excluded.peak_threshold,
        peak_spacing = excluded.peak_spacing
      ",
        params![
            session_id,
            reference_lap_id,
            compare_lap_id,
            smooth_lines.map(|v| if v { 1 } else { 0 }),
            show_legends.map(|v| if v { 1 } else { 0 }),
            race_line_color_mode,
            show_peaks.map(|v| if v { 1 } else { 0 }),
            peak_threshold,
            peak_spacing,
        ],
    )
    .map_err(|err| err.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_lap_samples(
    state: State<AppState>,
    lap_id: i64,
    limit: u32,
) -> Result<Vec<LapSamplePoint>, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "
      SELECT id, ts_ms, speed_kmh, throttle, brake, rpm
      FROM samples
      WHERE lap_id = ?1
      ORDER BY id ASC
      LIMIT ?2
      ",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![lap_id, limit], |row| {
            Ok(LapSamplePoint {
                idx: row.get(0)?,
                ts_ms: row.get(1)?,
                speed_kmh: row.get(2)?,
                throttle: row.get(3)?,
                brake: row.get(4)?,
                rpm: row.get(5)?,
            })
        })
        .map_err(|err| err.to_string())?;

    let mut points = Vec::new();
    for item in rows {
        points.push(item.map_err(|err| err.to_string())?);
    }
    Ok(points)
}

#[tauri::command]
fn get_lap_track_points(
    state: State<AppState>,
    lap_id: i64,
    limit: u32,
) -> Result<Vec<TrackPoint>, String> {
    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "
      SELECT x, z, throttle, brake
      FROM samples
      WHERE lap_id = ?1
      ORDER BY id ASC
      LIMIT ?2
      ",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map(params![lap_id, limit], |row| {
            Ok(TrackPoint {
                x: row.get(0)?,
                z: row.get(1)?,
                throttle: row.get(2)?,
                brake: row.get(3)?,
            })
        })
        .map_err(|err| err.to_string())?;

    let mut points = Vec::new();
    for item in rows {
        points.push(item.map_err(|err| err.to_string())?);
    }
    Ok(points)
}

#[tauri::command]
fn export_session_snapshot(
    state: State<AppState>,
    max_samples_per_lap: u32,
) -> Result<String, String> {
    let session_id = *state.shared.current_session_id.lock().unwrap();
    let Some(session_id) = session_id else {
        return Err("No active session".to_string());
    };

    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    let preferences = read_session_preferences(&conn, session_id).map_err(|err| err.to_string())?;

    let mut laps_stmt = conn
        .prepare(
            "
      SELECT id, lap_index, lap_time_ms
      FROM laps
      WHERE session_id = ?1
      ORDER BY lap_index ASC
      ",
        )
        .map_err(|err| err.to_string())?;

    let lap_rows = laps_stmt
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)? as i32,
                row.get(2)?,
            ))
        })
        .map_err(|err| err.to_string())?;

    let mut laps = Vec::new();
    for row in lap_rows {
        let (lap_id, lap_index, lap_time_ms) = row.map_err(|err| err.to_string())?;
        let samples = export_lap_samples(&conn, lap_id, max_samples_per_lap)?;
        laps.push(ExportLap {
            id: lap_id,
            lap_index,
            lap_time_ms,
            samples,
        });
    }

    let snapshot = SessionExport {
        session_id,
        preferences,
        laps,
    };

    serde_json::to_string_pretty(&snapshot).map_err(|err| err.to_string())
}

#[tauri::command]
fn import_session_snapshot(state: State<AppState>, json: String) -> Result<i64, String> {
    let snapshot: SessionExport = serde_json::from_str(&json).map_err(|err| err.to_string())?;

    let db_path = state
        .db_path
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Database path not initialized".to_string())?;
    init_database_at_path(&db_path).map_err(|err| err.to_string())?;
    let mut conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let tx = conn.transaction().map_err(|err| err.to_string())?;

    let session_id = insert_session_row(&tx).map_err(|err| err.to_string())?;

    tx.execute(
        "
    INSERT INTO session_preferences (
      session_id,
      reference_lap_id,
      compare_lap_id,
      smooth_lines,
      show_legends,
      race_line_color_mode,
      show_peaks,
      peak_threshold,
      peak_spacing
    )
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
    ",
        params![
            session_id,
            snapshot.preferences.reference_lap_id,
            snapshot.preferences.compare_lap_id,
            snapshot.preferences.smooth_lines,
            snapshot.preferences.show_legends,
            snapshot.preferences.race_line_color_mode,
            snapshot.preferences.show_peaks,
            snapshot.preferences.peak_threshold,
            snapshot.preferences.peak_spacing,
        ],
    )
    .map_err(|err| err.to_string())?;

    for lap in snapshot.laps {
        let lap_id = insert_lap_row(&tx, session_id, lap.lap_index, lap.lap_time_ms)
            .map_err(|err| err.to_string())?;
        for sample in lap.samples {
            insert_export_sample(&tx, lap_id, &sample).map_err(|err| err.to_string())?;
        }
    }

    tx.commit().map_err(|err| err.to_string())?;
    *state.shared.current_session_id.lock().unwrap() = Some(session_id);
    Ok(session_id)
}

fn now_millis() -> i64 {
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_packet_meta(buf: &[u8], size: usize) -> PacketMeta {
    PacketMeta {
        magic: read_u32_le(buf, 0, size),
        packet_id: None,
        payload_len: size,
    }
}

fn send_heartbeat(socket: &UdpSocket, ip: &str) -> Result<(), String> {
    let heartbeat = b"A";
    let target_primary = format!("{}:33739", ip);
    socket
        .send_to(heartbeat, target_primary)
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn send_broadcast_heartbeat(socket: &UdpSocket) -> Result<(), String> {
    let heartbeat = b"A";
    socket
        .send_to(heartbeat, "255.255.255.255:33739")
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

#[derive(Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Copy)]
struct TelemetrySample {
    packet_id: i32,
    lap_count: i16,
    laps_in_race: i16,
    best_lap_time_ms: i32,
    last_lap_time_ms: i32,
    time_of_day_ms: i32,
    pre_race_pos: i16,
    num_cars_pre_race: i16,
    min_alert_rpm: i16,
    max_alert_rpm: i16,
    calculated_max_speed: i16,
    flags: i16,
    current_gear: u8,
    suggested_gear: u8,
    throttle: u8,
    brake: u8,
    engine_rpm: f32,
    meters_per_second: f32,
    turbo_boost: f32,
    oil_pressure: f32,
    water_temperature: f32,
    oil_temperature: f32,
    gas_level: f32,
    gas_capacity: f32,
    body_height: f32,
    road_plane_distance: f32,
    position: Vec3,
    velocity: Vec3,
    rotation_pitch: f32,
    rotation_yaw: f32,
    rotation_roll: f32,
    angular_velocity: Vec3,
    road_plane: Vec3,
    tire_surface_temp: [f32; 4],
    wheel_rps: [f32; 4],
    tire_radius: [f32; 4],
    tire_sus_height: [f32; 4],
    clutch_pedal: f32,
    clutch_engagement: f32,
    rpm_from_clutch: f32,
    transmission_top_speed: f32,
    gear_ratios: [f32; 7],
    car_code: i32,
    wheel_rotation: Option<f32>,
    sway: Option<f32>,
    heave: Option<f32>,
    surge: Option<f32>,
    energy_recovery: Option<f32>,
}

struct PacketReader<'a> {
    buf: &'a [u8],
    pos: usize,
    size: usize,
    endian: Endian,
}

impl<'a> PacketReader<'a> {
    fn new(buf: &'a [u8], size: usize, endian: Endian) -> Self {
        Self {
            buf,
            pos: 0,
            size,
            endian,
        }
    }

    fn set_endian(&mut self, endian: Endian) {
        self.endian = endian;
    }

    fn skip(&mut self, bytes: usize) -> Result<(), String> {
        if self.pos + bytes > self.size {
            return Err("Buffer underrun while skipping".to_string());
        }
        self.pos += bytes;
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, String> {
        if self.pos + 1 > self.size {
            return Err("Buffer underrun while reading u8".to_string());
        }
        let value = self.buf[self.pos];
        self.pos += 1;
        Ok(value)
    }

    fn read_i16(&mut self) -> Result<i16, String> {
        let value = self.read_u16()?;
        Ok(value as i16)
    }

    fn read_u16(&mut self) -> Result<u16, String> {
        if self.pos + 2 > self.size {
            return Err("Buffer underrun while reading u16".to_string());
        }
        let bytes = [self.buf[self.pos], self.buf[self.pos + 1]];
        self.pos += 2;
        Ok(match self.endian {
            Endian::Little => u16::from_le_bytes(bytes),
            Endian::Big => u16::from_be_bytes(bytes),
        })
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        let value = self.read_u32()?;
        Ok(value as i32)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.size {
            return Err("Buffer underrun while reading u32".to_string());
        }
        let bytes = [
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ];
        self.pos += 4;
        Ok(match self.endian {
            Endian::Little => u32::from_le_bytes(bytes),
            Endian::Big => u32::from_be_bytes(bytes),
        })
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        let bits = self.read_u32()?;
        Ok(f32::from_bits(bits))
    }

    fn read_vec3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3 {
            x: self.read_f32()?,
            y: self.read_f32()?,
            z: self.read_f32()?,
        })
    }
}

fn parse_telemetry_sample(buf: &[u8], size: usize) -> Result<TelemetrySample, String> {
    if size < 4 {
        return Err("Packet too small".to_string());
    }

    let magic = read_u32_le(buf, 0, size).ok_or_else(|| "Missing magic".to_string())?;
    let endian = if magic == 0x30533647 {
        Endian::Big
    } else if magic == 0x47375330 {
        Endian::Little
    } else {
        return Err(format!("Unexpected magic {magic:#x}"));
    };

    let mut reader = PacketReader::new(buf, size, endian);
    let _magic = reader.read_u32()?;

    let position = reader.read_vec3()?;
    let velocity = reader.read_vec3()?;
    let rotation_x = reader.read_f32()?;
    let rotation_y = reader.read_f32()?;
    let rotation_z = reader.read_f32()?;
    let rotation_w = reader.read_f32()?;
    let (rotation_pitch, rotation_yaw, rotation_roll) =
        quat_to_euler(rotation_x, rotation_y, rotation_z, rotation_w);
    let angular_velocity = reader.read_vec3()?;
    let body_height = reader.read_f32()?;
    let engine_rpm = reader.read_f32()?;
    reader.skip(4)?; // IV
    let gas_level = reader.read_f32()?;
    let gas_capacity = reader.read_f32()?;
    let meters_per_second = reader.read_f32()?;
    let turbo_boost = reader.read_f32()?;
    let oil_pressure = reader.read_f32()?;
    let water_temperature = reader.read_f32()?;
    let oil_temperature = reader.read_f32()?;
    let tire_surface_temp = [
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ];
    let packet_id = reader.read_i32()?;
    let lap_count = reader.read_i16()?;
    let laps_in_race = reader.read_i16()?;
    let best_lap_time_ms = reader.read_i32()?;
    let last_lap_time_ms = reader.read_i32()?;
    let time_of_day_ms = reader.read_i32()?;
    let pre_race_pos = reader.read_i16()?;
    let num_cars_pre_race = reader.read_i16()?;
    let min_alert_rpm = reader.read_i16()?;
    let max_alert_rpm = reader.read_i16()?;
    let calculated_max_speed = reader.read_i16()?;
    let flags = reader.read_i16()?;

    let bits = reader.read_u8()?;
    let current_gear = bits & 0b1111;
    let suggested_gear = bits >> 4;
    let throttle = reader.read_u8()?;
    let brake = reader.read_u8()?;
    reader.skip(1)?;

    let road_plane = reader.read_vec3()?;
    let road_plane_distance = reader.read_f32()?;

    let wheel_rps = [
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ];
    let tire_radius = [
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ];
    let tire_sus_height = [
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
        reader.read_f32()?,
    ];

    reader.skip(4 * 8)?;

    let clutch_pedal = reader.read_f32()?;
    let clutch_engagement = reader.read_f32()?;
    let rpm_from_clutch = reader.read_f32()?;
    let transmission_top_speed = reader.read_f32()?;

    let mut gear_ratios = [0.0f32; 7];
    for ratio in &mut gear_ratios {
        *ratio = reader.read_f32()?;
    }
    let _gear_ratio_8 = reader.read_f32()?;
    let car_code = reader.read_i32()?;

    let mut wheel_rotation = None;
    let mut sway = None;
    let mut heave = None;
    let mut surge = None;
    let mut energy_recovery = None;

    if size >= 0x13C {
        wheel_rotation = Some(reader.read_f32()?);
        let _filler = reader.read_f32()?;
        sway = Some(reader.read_f32()?);
        heave = Some(reader.read_f32()?);
        surge = Some(reader.read_f32()?);
    }

    if size >= 0x158 {
        let _unk1 = reader.read_u8()?;
        let _unk2 = reader.read_u8()?;
        let _unk3 = reader.read_u8()?;
        let _no_gas = reader.read_u8()?;
        let _unk5 = [
            reader.read_f32()?,
            reader.read_f32()?,
            reader.read_f32()?,
            reader.read_f32()?,
        ];
        energy_recovery = Some(reader.read_f32()?);
        let _unk7 = reader.read_f32()?;
    }

    Ok(TelemetrySample {
        packet_id,
        lap_count,
        laps_in_race,
        best_lap_time_ms,
        last_lap_time_ms,
        time_of_day_ms,
        pre_race_pos,
        num_cars_pre_race,
        min_alert_rpm,
        max_alert_rpm,
        calculated_max_speed,
        flags,
        current_gear,
        suggested_gear,
        throttle,
        brake,
        engine_rpm,
        meters_per_second,
        turbo_boost,
        oil_pressure,
        water_temperature,
        oil_temperature,
        gas_level,
        gas_capacity,
        body_height,
        road_plane_distance,
        position,
        velocity,
        rotation_pitch,
        rotation_yaw,
        rotation_roll,
        angular_velocity,
        road_plane,
        tire_surface_temp,
        wheel_rps,
        tire_radius,
        tire_sus_height,
        clutch_pedal,
        clutch_engagement,
        rpm_from_clutch,
        transmission_top_speed,
        gear_ratios,
        car_code,
        wheel_rotation,
        sway,
        heave,
        surge,
        energy_recovery,
    })
}

fn quat_to_euler(x: f32, y: f32, z: f32, w: f32) -> (f32, f32, f32) {
    let sinr_cosp = 2.0 * (w * x + y * z);
    let cosr_cosp = 1.0 - 2.0 * (x * x + y * y);
    let roll = sinr_cosp.atan2(cosr_cosp);

    let mut sinp = 2.0 * (w * y - z * x);
    if sinp > 1.0 {
        sinp = 1.0;
    } else if sinp < -1.0 {
        sinp = -1.0;
    }
    let pitch = sinp.asin();

    let siny_cosp = 2.0 * (w * z + x * y);
    let cosy_cosp = 1.0 - 2.0 * (y * y + z * z);
    let yaw = siny_cosp.atan2(cosy_cosp);

    (pitch, yaw, roll)
}

struct Salsa20 {
    state: [u32; 16],
}

impl Salsa20 {
    fn new(key: &[u8]) -> Self {
        let constants = b"expand 32-byte k";
        let mut state = [0u32; 16];

        let key_words = key
            .chunks(4)
            .take(8)
            .map(|chunk| u32::from_le_bytes(pad4(chunk)))
            .collect::<Vec<_>>();

        state[0] = u32::from_le_bytes([constants[0], constants[1], constants[2], constants[3]]);
        state[5] = u32::from_le_bytes([constants[4], constants[5], constants[6], constants[7]]);
        state[10] = u32::from_le_bytes([constants[8], constants[9], constants[10], constants[11]]);
        state[15] =
            u32::from_le_bytes([constants[12], constants[13], constants[14], constants[15]]);

        for (i, word) in key_words.iter().enumerate() {
            if i < 4 {
                state[1 + i] = *word;
            } else {
                state[11 + (i - 4)] = *word;
            }
        }

        state[6] = 0;
        state[7] = 0;
        state[8] = 0;
        state[9] = 0;

        Self { state }
    }

    fn set_iv(&mut self, iv: &[u8; 8]) {
        self.state[6] = u32::from_le_bytes([iv[0], iv[1], iv[2], iv[3]]);
        self.state[7] = u32::from_le_bytes([iv[4], iv[5], iv[6], iv[7]]);
        self.state[8] = 0;
        self.state[9] = 0;
    }

    fn decrypt(&mut self, data: &mut [u8]) {
        let mut offset = 0;
        while offset < data.len() {
            let block = self.hash();
            self.increment();
            let remaining = data.len() - offset;
            let chunk = remaining.min(64);
            for i in 0..chunk {
                data[offset + i] ^= block[i];
            }
            offset += chunk;
        }
    }

    fn increment(&mut self) {
        self.state[8] = self.state[8].wrapping_add(1);
        if self.state[8] == 0 {
            self.state[9] = self.state[9].wrapping_add(1);
        }
    }

    fn hash(&self) -> [u8; 64] {
        let mut working = self.state;
        for _ in 0..10 {
            quarter_round(&mut working, 0, 4, 8, 12);
            quarter_round(&mut working, 5, 9, 13, 1);
            quarter_round(&mut working, 10, 14, 2, 6);
            quarter_round(&mut working, 15, 3, 7, 11);

            quarter_round(&mut working, 0, 1, 2, 3);
            quarter_round(&mut working, 5, 6, 7, 4);
            quarter_round(&mut working, 10, 11, 8, 9);
            quarter_round(&mut working, 15, 12, 13, 14);
        }

        for i in 0..16 {
            working[i] = working[i].wrapping_add(self.state[i]);
        }

        let mut out = [0u8; 64];
        for (i, word) in working.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }
}

fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[b] ^= state[a].wrapping_add(state[d]).rotate_left(7);
    state[c] ^= state[b].wrapping_add(state[a]).rotate_left(9);
    state[d] ^= state[c].wrapping_add(state[b]).rotate_left(13);
    state[a] ^= state[d].wrapping_add(state[c]).rotate_left(18);
}

fn pad4(input: &[u8]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (i, byte) in input.iter().enumerate().take(4) {
        out[i] = *byte;
    }
    out
}

fn read_u32_le(buf: &[u8], offset: usize, size: usize) -> Option<u32> {
    if size < offset + 4 {
        return None;
    }
    let bytes = [
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ];
    Some(u32::from_le_bytes(bytes))
}

fn decrypt_gt7_packet(payload: &mut [u8]) -> Result<(), String> {
    if payload.len() < 0x44 {
        return Err("Packet too small to decrypt".to_string());
    }
    let iv1 = read_u32_le(payload, 0x40, payload.len()).ok_or_else(|| "Missing IV".to_string())?;
    let xor_key = 0xDEADBEAF_u32;
    let iv2 = iv1 ^ xor_key;
    let mut iv = [0u8; 8];
    iv[..4].copy_from_slice(&iv2.to_le_bytes());
    iv[4..].copy_from_slice(&iv1.to_le_bytes());

    let mut salsa = Salsa20::new(b"Simulator Interface Packet GT7 ver 0.0");
    salsa.set_iv(&iv);
    salsa.decrypt(payload);
    let magic =
        read_u32_le(payload, 0, payload.len()).ok_or_else(|| "Missing magic".to_string())?;
    if magic != 0x47375330 {
        return Err("Invalid magic after decrypt".to_string());
    }
    Ok(())
}

fn init_database_at_path(path: &PathBuf) -> rusqlite::Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;

    CREATE TABLE IF NOT EXISTS sessions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      started_at INTEGER NOT NULL,
      track_id INTEGER,
      car_id INTEGER,
      notes TEXT,
      is_replay INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS laps (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      session_id INTEGER NOT NULL,
      lap_index INTEGER NOT NULL,
      lap_time_ms INTEGER,
      is_valid INTEGER NOT NULL DEFAULT 1,
      is_replay INTEGER NOT NULL DEFAULT 0,
      FOREIGN KEY(session_id) REFERENCES sessions(id)
    );

    CREATE TABLE IF NOT EXISTS samples (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      lap_id INTEGER NOT NULL,
      ts_ms INTEGER NOT NULL,
      dist_m REAL,
      speed_kmh REAL,
      throttle REAL,
      brake REAL,
      rpm REAL,
      gear INTEGER,
      suggested_gear INTEGER,
      yaw_rate REAL,
      x REAL,
      y REAL,
      z REAL,
      velocity_x REAL,
      velocity_y REAL,
      velocity_z REAL,
      angular_velocity_x REAL,
      angular_velocity_y REAL,
      angular_velocity_z REAL,
      rotation_pitch REAL,
      rotation_yaw REAL,
      rotation_roll REAL,
      fuel REAL,
      fuel_capacity REAL,
      fuel_pct REAL,
      body_height REAL,
      oil_temp REAL,
      water_temp REAL,
      oil_pressure REAL,
      tire_temp_fl REAL,
      tire_temp_fr REAL,
      tire_temp_rl REAL,
      tire_temp_rr REAL,
      boost REAL,
      clutch REAL,
      clutch_engagement REAL,
      rpm_from_clutch REAL,
      lap_count INTEGER,
      laps_in_race INTEGER,
      current_lap_time_ms INTEGER,
      last_lap_time_ms INTEGER,
      best_lap_time_ms INTEGER,
      time_of_day_ms INTEGER,
      pre_race_pos INTEGER,
      num_cars_pre_race INTEGER,
      min_alert_rpm INTEGER,
      max_alert_rpm INTEGER,
      calculated_max_speed INTEGER,
      car_code INTEGER,
      flags INTEGER,
      asm_active INTEGER,
      tcs_active INTEGER,
      rev_limiter_active INTEGER,
      car_on_track INTEGER,
      paused INTEGER,
      loading_or_processing INTEGER,
      FOREIGN KEY(lap_id) REFERENCES laps(id)
    );

    CREATE TABLE IF NOT EXISTS cars (
      id INTEGER PRIMARY KEY,
      name TEXT NOT NULL,
      source TEXT
    );

    CREATE TABLE IF NOT EXISTS tracks (
      id INTEGER PRIMARY KEY,
      name TEXT NOT NULL,
      source TEXT
    );

    CREATE TABLE IF NOT EXISTS session_preferences (
      session_id INTEGER PRIMARY KEY,
      reference_lap_id INTEGER,
      compare_lap_id INTEGER,
      smooth_lines INTEGER,
      show_legends INTEGER,
      race_line_color_mode TEXT,
      show_peaks INTEGER,
      peak_threshold INTEGER,
      peak_spacing INTEGER,
      FOREIGN KEY(session_id) REFERENCES sessions(id)
    );

    CREATE INDEX IF NOT EXISTS idx_laps_session_id ON laps(session_id);
    CREATE INDEX IF NOT EXISTS idx_samples_lap_id ON samples(lap_id);
    CREATE INDEX IF NOT EXISTS idx_samples_ts_ms ON samples(ts_ms);
    CREATE INDEX IF NOT EXISTS idx_samples_dist_m ON samples(dist_m);
    ",
    )?;

    ensure_column(&conn, "session_preferences", "smooth_lines", "INTEGER")?;
    ensure_column(&conn, "session_preferences", "show_legends", "INTEGER")?;
    ensure_column(&conn, "session_preferences", "race_line_color_mode", "TEXT")?;
    ensure_column(&conn, "session_preferences", "show_peaks", "INTEGER")?;
    ensure_column(&conn, "session_preferences", "peak_threshold", "INTEGER")?;
    ensure_column(&conn, "session_preferences", "peak_spacing", "INTEGER")?;
    ensure_column(&conn, "samples", "suggested_gear", "INTEGER")?;
    ensure_column(&conn, "samples", "velocity_x", "REAL")?;
    ensure_column(&conn, "samples", "velocity_y", "REAL")?;
    ensure_column(&conn, "samples", "velocity_z", "REAL")?;
    ensure_column(&conn, "samples", "angular_velocity_x", "REAL")?;
    ensure_column(&conn, "samples", "angular_velocity_y", "REAL")?;
    ensure_column(&conn, "samples", "angular_velocity_z", "REAL")?;
    ensure_column(&conn, "samples", "rotation_pitch", "REAL")?;
    ensure_column(&conn, "samples", "rotation_yaw", "REAL")?;
    ensure_column(&conn, "samples", "rotation_roll", "REAL")?;
    ensure_column(&conn, "samples", "fuel_capacity", "REAL")?;
    ensure_column(&conn, "samples", "fuel_pct", "REAL")?;
    ensure_column(&conn, "samples", "oil_temp", "REAL")?;
    ensure_column(&conn, "samples", "water_temp", "REAL")?;
    ensure_column(&conn, "samples", "oil_pressure", "REAL")?;
    ensure_column(&conn, "samples", "tire_temp_fl", "REAL")?;
    ensure_column(&conn, "samples", "tire_temp_fr", "REAL")?;
    ensure_column(&conn, "samples", "tire_temp_rl", "REAL")?;
    ensure_column(&conn, "samples", "tire_temp_rr", "REAL")?;
    ensure_column(&conn, "samples", "boost", "REAL")?;
    ensure_column(&conn, "samples", "clutch", "REAL")?;
    ensure_column(&conn, "samples", "clutch_engagement", "REAL")?;
    ensure_column(&conn, "samples", "rpm_from_clutch", "REAL")?;
    ensure_column(&conn, "samples", "lap_count", "INTEGER")?;
    ensure_column(&conn, "samples", "laps_in_race", "INTEGER")?;
    ensure_column(&conn, "samples", "current_lap_time_ms", "INTEGER")?;
    ensure_column(&conn, "samples", "last_lap_time_ms", "INTEGER")?;
    ensure_column(&conn, "samples", "best_lap_time_ms", "INTEGER")?;
    ensure_column(&conn, "samples", "time_of_day_ms", "INTEGER")?;
    ensure_column(&conn, "samples", "pre_race_pos", "INTEGER")?;
    ensure_column(&conn, "samples", "num_cars_pre_race", "INTEGER")?;
    ensure_column(&conn, "samples", "min_alert_rpm", "INTEGER")?;
    ensure_column(&conn, "samples", "max_alert_rpm", "INTEGER")?;
    ensure_column(&conn, "samples", "calculated_max_speed", "INTEGER")?;
    ensure_column(&conn, "samples", "car_code", "INTEGER")?;
    ensure_column(&conn, "samples", "asm_active", "INTEGER")?;
    ensure_column(&conn, "samples", "tcs_active", "INTEGER")?;
    ensure_column(&conn, "samples", "rev_limiter_active", "INTEGER")?;
    ensure_column(&conn, "samples", "car_on_track", "INTEGER")?;
    ensure_column(&conn, "samples", "paused", "INTEGER")?;
    ensure_column(&conn, "samples", "loading_or_processing", "INTEGER")?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    column_type: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in rows {
        if name? == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}"),
        [],
    )?;
    Ok(())
}

fn create_session(conn: &Connection) -> rusqlite::Result<i64> {
    let started_at = now_millis();
    conn.execute(
        "
    INSERT INTO sessions (started_at, track_id, car_id, notes, is_replay)
    VALUES (?1, NULL, NULL, NULL, 0)
    ",
        [started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_session_row(conn: &Connection) -> rusqlite::Result<i64> {
    let started_at = now_millis();
    conn.execute(
        "
    INSERT INTO sessions (started_at, track_id, car_id, notes, is_replay)
    VALUES (?1, NULL, NULL, NULL, 0)
    ",
        [started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

fn create_lap(conn: &Connection, session_id: i64, lap_index: i32) -> rusqlite::Result<i64> {
    conn.execute(
        "
    INSERT INTO laps (session_id, lap_index, lap_time_ms, is_valid, is_replay)
    VALUES (?1, ?2, NULL, 1, 0)
    ",
        params![session_id, lap_index],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_lap_row(
    conn: &Connection,
    session_id: i64,
    lap_index: i32,
    lap_time_ms: Option<i64>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "
    INSERT INTO laps (session_id, lap_index, lap_time_ms, is_valid, is_replay)
    VALUES (?1, ?2, ?3, 1, 0)
    ",
        params![session_id, lap_index, lap_time_ms],
    )?;
    Ok(conn.last_insert_rowid())
}

fn update_lap_time(conn: &Connection, lap_id: i64, lap_time_ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE laps SET lap_time_ms = ?1 WHERE id = ?2",
        params![lap_time_ms, lap_id],
    )?;
    Ok(())
}

fn insert_sample(
    conn: &Connection,
    lap_id: i64,
    sample: &TelemetrySample,
    ts_ms: i64,
    dist_m: f64,
    current_lap_time_ms: i32,
    asm_active: bool,
    tcs_active: bool,
    rev_limiter_active: bool,
    car_on_track: bool,
    paused: bool,
    loading_or_processing: bool,
) -> rusqlite::Result<()> {
    let speed_kmh = sample.meters_per_second as f64 * 3.6;
    let fuel_pct = if sample.gas_capacity > 0.0 {
        (sample.gas_level / sample.gas_capacity) * 100.0
    } else {
        0.0
    };
    conn.execute(
        "
    INSERT INTO samples (
      lap_id, ts_ms, dist_m, speed_kmh, throttle, brake, rpm, gear, suggested_gear, yaw_rate,
      x, y, z, velocity_x, velocity_y, velocity_z, angular_velocity_x, angular_velocity_y,
      angular_velocity_z, rotation_pitch, rotation_yaw, rotation_roll, fuel, fuel_capacity,
      fuel_pct, body_height, oil_temp, water_temp, oil_pressure, tire_temp_fl, tire_temp_fr,
      tire_temp_rl, tire_temp_rr, boost, clutch, clutch_engagement, rpm_from_clutch, lap_count,
      laps_in_race, current_lap_time_ms, last_lap_time_ms, best_lap_time_ms, time_of_day_ms,
      pre_race_pos, num_cars_pre_race, min_alert_rpm, max_alert_rpm, calculated_max_speed,
      car_code, flags, asm_active, tcs_active, rev_limiter_active, car_on_track, paused,
      loading_or_processing
    )
    VALUES (
      ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
      ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
      ?19, ?20, ?21, ?22, ?23, ?24,
      ?25, ?26, ?27, ?28, ?29, ?30, ?31,
      ?32, ?33, ?34, ?35, ?36, ?37, ?38,
      ?39, ?40, ?41, ?42, ?43, ?44,
      ?45, ?46, ?47, ?48, ?49,
      ?50, ?51, ?52, ?53, ?54, ?55,
      ?56
    )
    ",
        params![
            lap_id,
            ts_ms,
            dist_m,
            speed_kmh,
            sample.throttle as f64 / 2.55,
            sample.brake as f64 / 2.55,
            sample.engine_rpm as f64,
            sample.current_gear as i32,
            sample.suggested_gear as i32,
            sample.angular_velocity.y as f64,
            sample.position.x as f64,
            sample.position.y as f64,
            sample.position.z as f64,
            sample.velocity.x as f64,
            sample.velocity.y as f64,
            sample.velocity.z as f64,
            sample.angular_velocity.x as f64,
            sample.angular_velocity.y as f64,
            sample.angular_velocity.z as f64,
            sample.rotation_pitch as f64,
            sample.rotation_yaw as f64,
            sample.rotation_roll as f64,
            sample.gas_level as f64,
            sample.gas_capacity as f64,
            fuel_pct as f64,
            sample.body_height as f64,
            sample.oil_temperature as f64,
            sample.water_temperature as f64,
            sample.oil_pressure as f64,
            sample.tire_surface_temp[0] as f64,
            sample.tire_surface_temp[1] as f64,
            sample.tire_surface_temp[2] as f64,
            sample.tire_surface_temp[3] as f64,
            sample.turbo_boost as f64,
            sample.clutch_pedal as f64,
            sample.clutch_engagement as f64,
            sample.rpm_from_clutch as f64,
            sample.lap_count as i32,
            sample.laps_in_race as i32,
            current_lap_time_ms as i32,
            sample.last_lap_time_ms as i32,
            sample.best_lap_time_ms as i32,
            sample.time_of_day_ms as i32,
            sample.pre_race_pos as i32,
            sample.num_cars_pre_race as i32,
            sample.min_alert_rpm as i32,
            sample.max_alert_rpm as i32,
            sample.calculated_max_speed as i32,
            sample.car_code as i32,
            sample.flags as i32,
            asm_active as i32,
            tcs_active as i32,
            rev_limiter_active as i32,
            car_on_track as i32,
            paused as i32,
            loading_or_processing as i32,
        ],
    )?;
    Ok(())
}

fn insert_export_sample(
    conn: &Connection,
    lap_id: i64,
    sample: &ExportSample,
) -> rusqlite::Result<()> {
    conn.execute(
        "
    INSERT INTO samples (
      lap_id, ts_ms, dist_m, speed_kmh, throttle, brake, rpm, gear, yaw_rate,
      x, y, z, fuel, body_height, flags
    )
    VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, NULL, ?8, NULL, ?9, NULL, NULL, NULL)
    ",
        params![
            lap_id,
            sample.ts_ms,
            sample.speed_kmh,
            sample.throttle,
            sample.brake,
            sample.rpm,
            sample.gear,
            sample.x,
            sample.z,
        ],
    )?;
    Ok(())
}

fn export_lap_samples(
    conn: &Connection,
    lap_id: i64,
    max_samples: u32,
) -> Result<Vec<ExportSample>, String> {
    let mut count_stmt = conn
        .prepare("SELECT COUNT(*) FROM samples WHERE lap_id = ?1")
        .map_err(|err| err.to_string())?;
    let total: i64 = count_stmt
        .query_row([lap_id], |row| row.get(0))
        .map_err(|err| err.to_string())?;

    let step = if total > max_samples as i64 && max_samples > 0 {
        (total as f64 / max_samples as f64).ceil() as i64
    } else {
        1
    };

    let mut stmt = conn
        .prepare(
            "
      SELECT ts_ms, speed_kmh, throttle, brake, rpm, gear, x, z
      FROM samples
      WHERE lap_id = ?1
      ORDER BY id ASC
      ",
        )
        .map_err(|err| err.to_string())?;

    let rows = stmt
        .query_map([lap_id], |row| {
            Ok(ExportSample {
                ts_ms: row.get(0)?,
                speed_kmh: row.get(1)?,
                throttle: row.get(2)?,
                brake: row.get(3)?,
                rpm: row.get(4)?,
                gear: row.get(5)?,
                x: row.get(6)?,
                z: row.get(7)?,
            })
        })
        .map_err(|err| err.to_string())?;

    let mut samples = Vec::new();
    let mut idx = 0i64;
    for row in rows {
        let sample = row.map_err(|err| err.to_string())?;
        if idx % step == 0 {
            samples.push(sample);
        }
        idx += 1;
    }
    Ok(samples)
}

fn read_session_preferences(
    conn: &Connection,
    session_id: i64,
) -> rusqlite::Result<SessionPreferences> {
    let mut stmt = conn.prepare(
    "
    SELECT reference_lap_id, compare_lap_id, smooth_lines, show_legends, race_line_color_mode, show_peaks,
           peak_threshold, peak_spacing
    FROM session_preferences
    WHERE session_id = ?1
    ",
  )?;

    let mut rows = stmt.query([session_id])?;
    if let Some(row) = rows.next()? {
        Ok(SessionPreferences {
            reference_lap_id: row.get(0).ok(),
            compare_lap_id: row.get(1).ok(),
            smooth_lines: row.get(2).ok(),
            show_legends: row.get(3).ok(),
            race_line_color_mode: row.get(4).ok(),
            show_peaks: row.get(5).ok(),
            peak_threshold: row.get(6).ok(),
            peak_spacing: row.get(7).ok(),
        })
    } else {
        Ok(SessionPreferences {
            reference_lap_id: None,
            compare_lap_id: None,
            smooth_lines: None,
            show_legends: None,
            race_line_color_mode: None,
            show_peaks: None,
            peak_threshold: None,
            peak_spacing: None,
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            shared: Arc::new(SharedState::default()),
            stop_tx: Mutex::new(None),
            db_path: Mutex::new(None),
            target_ip: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            get_app_status,
            get_live_payload,
            get_database_info,
            vacuum_database,
            reset_database,
            delete_lap,
            delete_session,
            start_listener,
            stop_listener,
            init_database,
            set_target_ip,
            get_recent_samples,
            list_laps,
            list_sessions,
            set_current_session,
            get_lap_samples,
            get_lap_track_points,
            get_session_preferences,
            set_session_preferences,
            export_session_snapshot,
            import_session_snapshot
        ])
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db_path = app_data_dir.join("gt7-telemetry.sqlite");
            let state = app.state::<AppState>();
            *state.db_path.lock().unwrap() = Some(db_path);

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
