#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adslicer;

use adslicer::job::{cancel_current_job, run_job, JobParams};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Window};

static JOB_RUNNING: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

// ─── Preset metadata ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PresetMeta {
    /// Filename stem, e.g. "vhs_noisy"
    pub name: String,
    /// Full absolute path on disk
    pub path: String,
    /// Value of "_preset" key in JSON, or the filename stem
    pub label: String,
    /// Value of "_description" key in JSON, or empty string
    pub description: String,
    /// true = shipped inside the app bundle; false = user-imported
    pub builtin: bool,
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

/// The bundled presets directory — inside the app bundle, read-only.
/// Tauri 2: resource_dir() resolves to the Resources/ folder in the .app on
/// macOS, the install directory on Windows, and ~/.local/share/<id> on Linux.
/// We declared  "resources": { "presets/*": "presets" }  in tauri.conf.json,
/// so the preset files land at {resource_dir}/presets/*.json.
fn bundled_presets_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|d| d.join("presets"))
}

/// The user presets directory — writable, lives in the app config dir.
/// Created on first use.
fn user_presets_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path()
        .app_config_dir()
        .map_err(|e| format!("Cannot resolve config dir: {e}"))?
        .join("presets");
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create user presets dir: {e}"))?;
    Ok(dir)
}

// ─── Shared preset-reading logic ──────────────────────────────────────────────

fn read_presets_from_dir(dir: &PathBuf, builtin: bool) -> Vec<PresetMeta> {
    let mut metas = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return metas,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        let name = path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let raw = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or(serde_json::Value::Object(Default::default()));

        let label = parsed.get("_preset")
            .and_then(|v| v.as_str())
            .unwrap_or(&name)
            .to_string();

        let description = parsed.get("_description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        metas.push(PresetMeta {
            name: name.clone(),
            path: path.to_string_lossy().to_string(),
            label,
            description,
            builtin,
        });
    }

    metas
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// Return all available presets: bundled ones first, then user-imported.
/// Both sets sorted by name within their group.
#[tauri::command]
fn list_presets(app: AppHandle) -> Result<Vec<PresetMeta>, String> {
    let mut all: Vec<PresetMeta> = Vec::new();

    // Built-in presets from the app bundle (read-only)
    if let Some(dir) = bundled_presets_dir(&app) {
        let mut bundled = read_presets_from_dir(&dir, true);
        bundled.sort_by(|a, b| a.name.cmp(&b.name));
        all.extend(bundled);
    }

    // User-imported presets from config dir (read-write)
    let user_dir = user_presets_dir(&app)?;
    let mut user = read_presets_from_dir(&user_dir, false);
    user.sort_by(|a, b| a.name.cmp(&b.name));
    all.extend(user);

    Ok(all)
}

/// Read a preset file by its full path and return the raw JSON string.
#[tauri::command]
fn load_preset(path: String) -> Result<String, String> {
    fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read preset '{path}': {e}"))
}

/// Save the current UI params as a named preset in the user presets dir.
/// `filename` is sanitised — only alphanumeric, hyphens, underscores.
/// Returns the full path written.
#[tauri::command]
fn save_preset(app: AppHandle, filename: String, content: String) -> Result<String, String> {
    let safe: String = filename.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if safe.is_empty() {
        return Err("Preset name must not be empty".into());
    }

    let mut parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON: {e}"))?;

    if parsed.get("_preset").is_none() {
        if let serde_json::Value::Object(ref mut map) = parsed {
            map.insert(
                "_preset".to_string(),
                serde_json::Value::String(safe.replace('_', " ")),
            );
        }
    }

    let dir  = user_presets_dir(&app)?;
    let path = dir.join(format!("{}.json", safe));
    let out  = serde_json::to_string_pretty(&parsed)
        .map_err(|e| format!("Cannot serialise: {e}"))?;

    fs::write(&path, out)
        .map_err(|e| format!("Cannot write preset: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

/// Delete a user preset. Only files inside the user presets dir are accepted —
/// bundled presets cannot be deleted.
#[tauri::command]
fn delete_preset(app: AppHandle, path: String) -> Result<(), String> {
    let user_dir = user_presets_dir(&app)?;
    let target   = PathBuf::from(&path);

    if !target.starts_with(&user_dir) {
        return Err("Cannot delete a built-in preset".into());
    }
    if target.exists() {
        fs::remove_file(&target)
            .map_err(|e| format!("Cannot delete: {e}"))?;
    }
    Ok(())
}

/// Return the user presets folder path for display / shell-open.
#[tauri::command]
fn get_presets_dir(app: AppHandle) -> Result<String, String> {
    user_presets_dir(&app).map(|p| p.to_string_lossy().to_string())
}

// ─── Job commands ─────────────────────────────────────────────────────────────

#[tauri::command]
async fn run_adslicer_job(window: Window, params: JobParams) -> Result<(), String> {
    {
        let mut running = JOB_RUNNING.lock().unwrap();
        if *running { return Err("A job is already running".into()); }
        *running = true;
    }
    let win = window.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(err) = run_job(&win, params) {
            let _ = win.emit("adslicer-log", format!("[error] {err}"));
        } else {
            let _ = win.emit("adslicer-log", "[done] Job complete");
        }
        let mut running = JOB_RUNNING.lock().unwrap();
        *running = false;
    });
    Ok(())
}

#[tauri::command]
fn cancel_adslicer_job() -> Result<(), String> {
    cancel_current_job();
    Ok(())
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            run_adslicer_job,
            cancel_adslicer_job,
            list_presets,
            load_preset,
            save_preset,
            delete_preset,
            get_presets_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AdSlicerProXP");
}
