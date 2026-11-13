use fancypants::{load_config, xdg_config_path, Config, SessionHandle, SessionTelemetry};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

/// Android JNI entry point to initialize `btleplug`'s global JVM pointer so
/// both the Fancypants sensor client and the embedded Buttplug BLE server can
/// access Android's BluetoothAdapter from Rust threads.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_io_github_goopi34_fancypants_MainActivity_initBtleplug(
    env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
) {
    let _ = btleplug::platform::init(&env);
}

struct AppState {
    config: Mutex<Config>,
    config_path: Option<PathBuf>,
    session: Mutex<Option<SessionHandle>>,
    last_telemetry: Mutex<SessionTelemetry>,
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    state
        .config
        .lock()
        .map(|c| c.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    if let Some(ref path) = state.config_path {
        config.save(path).map_err(|e| format!("{:#}", e))?;
    }
    if let Ok(mut guard) = state.config.lock() {
        *guard = config.clone();
    }
    if let Ok(guard) = state.session.lock() {
        if let Some(ref handle) = *guard {
            handle
                .update_config(config)
                .map_err(|e| format!("{:#}", e))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn update_live_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    if let Ok(mut guard) = state.config.lock() {
        *guard = config.clone();
    }
    if let Ok(guard) = state.session.lock() {
        if let Some(ref handle) = *guard {
            handle
                .update_config(config)
                .map_err(|e| format!("{:#}", e))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn get_telemetry(state: State<'_, AppState>) -> Result<SessionTelemetry, String> {
    state
        .last_telemetry
        .lock()
        .map(|t| t.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Stop existing session if any
    let existing = state.session.lock().map_err(|e| e.to_string())?.take();
    if let Some(prev) = existing {
        prev.stop().await;
    }

    let config = state.config.lock().map_err(|e| e.to_string())?.clone();

    let app_clone = app.clone();
    let handle = SessionHandle::spawn(config, move |telemetry| {
        if let Some(st) = app_clone.try_state::<AppState>() {
            if let Ok(mut last) = st.last_telemetry.lock() {
                *last = telemetry.clone();
            }
        }
        let _ = app_clone.emit("telemetry", &telemetry);
    });

    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
async fn stop_session(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let existing = state.session.lock().map_err(|e| e.to_string())?.take();
    if let Some(handle) = existing {
        handle.stop().await;
    }
    let stopped = SessionTelemetry::default();
    if let Ok(mut last) = state.last_telemetry.lock() {
        *last = stopped.clone();
    }
    let _ = app.emit("telemetry", &stopped);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .setup(|app| {
            let config_path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join("config.toml"))
                .or_else(xdg_config_path);

            let initial_config = match config_path.as_deref() {
                Some(p) if p.exists() => load_config(Some(p)).unwrap_or_default(),
                _ => load_config(None).unwrap_or_default(),
            };

            app.manage(AppState {
                config: Mutex::new(initial_config),
                config_path,
                session: Mutex::new(None),
                last_telemetry: Mutex::new(SessionTelemetry::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            update_live_config,
            get_telemetry,
            start_session,
            stop_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fancypants application");
}
