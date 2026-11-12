// Orbiz : orbe flottant pour Windows. Un clic ouvre un anneau de raccourcis ; chat IA (OpenAI) qui pilote le navigateur
// (IrisBrowser) et les apps compatibles iris-link ; notes ; mini navigateur. Rust + Tauri 2, pages dans ui/.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod browser;
mod chat;
mod orb;
mod overlays;

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, Wry};

const CHAT_SHORTCUT: &str = "ctrl+shift+space";
const NOTES_SHORTCUT: &str = "ctrl+shift+n";
const BROWSER_SHORTCUT: &str = "ctrl+shift+b";

/// Données d'Orbiz : %APPDATA%\orbiz (le même dossier que l'ancienne version : notes, chat, dépenses gardés).
pub fn data_dir() -> PathBuf {
    let d = iris_store::data_dir("orbiz");
    let _ = std::fs::create_dir_all(&d);
    d
}
/// Dossier qui contient les apps Iris (IrisDJ, IrisBrowser…) : un parent de l'exécutable.
pub fn iris_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.ancestors()
        .find(|p| p.join("IrisBrowser").exists() || p.join("iris-briques").exists() || p.join("iris-link").exists())
        .map(PathBuf::from)
        .unwrap_or_else(|| exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).and_then(|p| p.parent()).map(PathBuf::from).unwrap_or_default())
}

// ------------------------------------------------------------ démarrage avec Windows (clé Run de l'utilisateur)
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
fn is_autostart() -> bool {
    std::process::Command::new("reg").args(["query", RUN_KEY, "/v", "Orbiz"]).output().map(|o| o.status.success()).unwrap_or(false)
}
fn set_autostart(on: bool) {
    let exe = std::env::current_exe().unwrap_or_default();
    let _ = if on {
        std::process::Command::new("reg").args(["add", RUN_KEY, "/v", "Orbiz", "/t", "REG_SZ", "/d", &format!("\"{}\"", exe.display()), "/f"]).output()
    } else {
        std::process::Command::new("reg").args(["delete", RUN_KEY, "/v", "Orbiz", "/f"]).output()
    };
}

// ------------------------------------------------------------ menu (clic droit sur l'orbe, icône de notification)
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("title", "Orbiz").enabled(false).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("recenter", "Recentrer l'orbe").build(app)?)
        .item(&MenuItemBuilder::with_id("chat", "ChatGPT").accelerator("Ctrl+Shift+Space").build(app)?)
        .item(&MenuItemBuilder::with_id("notes", "Notes").accelerator("Ctrl+Shift+N").build(app)?)
        .item(&MenuItemBuilder::with_id("browser", "Mini Browser").accelerator("Ctrl+Shift+B").build(app)?)
        .item(&CheckMenuItemBuilder::with_id("autostart", "Lancer au démarrage de Windows").checked(is_autostart()).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("quit", "Quitter").build(app)?)
        .build()
}
fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "recenter" => orb::recenter(app),
        "chat" | "notes" | "browser" => overlays::open(app, id),
        "autostart" => set_autostart(!is_autostart()),
        "quit" => animate_quit(app),
        _ => {}
    }
}

/// Petite icône ronde (dégradé bleu) pour la zone de notification.
fn tray_icon() -> tauri::image::Image<'static> {
    let s = 32usize;
    let c = (s as f64 - 1.0) / 2.0;
    let mut buf = vec![0u8; s * s * 4];
    for y in 0..s {
        for x in 0..s {
            let (fx, fy) = (x as f64, y as f64);
            let a = (14.5 - (fx - c).hypot(fy - c)).clamp(0.0, 1.0);
            let t = ((fx - c + 5.0).hypot(fy - c + 5.0) / 20.0).clamp(0.0, 1.0);
            let (r, g, b) = (200.0 - 110.0 * t, 215.0 - 145.0 * t, 255.0 - 25.0 * t);
            let i = (y * s + x) * 4;
            buf[i] = r as u8;
            buf[i + 1] = g as u8;
            buf[i + 2] = b as u8;
            buf[i + 3] = (255.0 * a) as u8;
        }
    }
    tauri::image::Image::new_owned(buf, s as u32, s as u32)
}

// ------------------------------------------------------------ fermeture animée
static QUITTING: AtomicBool = AtomicBool::new(false);
fn animate_quit(app: &AppHandle) {
    if QUITTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = app.emit_to("orb", "force-close", ());
    let _ = app.emit_to("orb", "quit-animation", ());
    // sécurité : on quitte même si la page ne répond pas
    let a = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2500));
        orb::save();
        a.exit(0);
    });
}

// ------------------------------------------------------------ commandes : orbe
#[tauri::command]
fn orb_actions() -> Value {
    actions::public_list()
}
#[tauri::command]
fn orb_run_action(app: AppHandle, id: String) {
    actions::run(&app, &id);
}
#[tauri::command]
fn orb_set_ignore(app: AppHandle, ignore: bool) {
    if let Some(w) = app.get_webview_window("orb") {
        let _ = w.set_ignore_cursor_events(ignore);
    }
}
#[tauri::command]
fn orb_drag_start(app: AppHandle) {
    orb::drag_start(&app);
}
#[tauri::command]
fn orb_drag_end() {
    orb::drag_end();
}
#[tauri::command]
fn orb_menu_open(app: AppHandle) {
    orb::menu_open(&app);
}
#[tauri::command]
fn orb_menu_close() {
    orb::menu_close();
}
/// Menu du clic droit. Windows referme aussitôt un menu dont la fenêtre n'est pas au premier plan :
/// on donne d'abord le focus à l'orbe (qui ne le prend jamais tout seul).
#[tauri::command]
fn orb_context_menu(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(w) = app.get_webview_window("orb") else { return };
        let _ = w.set_focus();
        std::thread::sleep(Duration::from_millis(80));
        if let Ok(m) = build_menu(&app) {
            let _ = w.popup_menu(&m);
        }
    });
}
#[tauri::command]
fn orb_quit_ready(app: AppHandle) {
    orb::save();
    app.exit(0);
}

// ------------------------------------------------------------ commandes : fenêtres flottantes
#[tauri::command]
fn overlay_hide(window: tauri::WebviewWindow) {
    let _ = window.hide();
}
/// Transparence de toute la fenêtre (mini navigateur), mémorisée.
#[tauri::command]
fn overlay_set_opacity(window: tauri::WebviewWindow, value: f64) {
    overlays::set_opacity(&window, value);
    overlays::opacity_save(window.label(), value);
}
/// Transparence du fond réglée par la page (chat, notes) : seulement mémorisée.
#[tauri::command]
fn overlay_save_opacity(window: tauri::WebviewWindow, value: f64) {
    overlays::opacity_save(window.label(), value);
}
#[tauri::command]
fn overlay_get_opacity(window: tauri::WebviewWindow) -> Option<f64> {
    overlays::opacity_get(window.label())
}

// ------------------------------------------------------------ commandes : chat
#[tauri::command]
fn chat_get_settings() -> Value {
    chat::get_settings()
}
#[tauri::command]
async fn chat_save_settings(s: Value) -> Result<Value, String> {
    chat::save_settings(s).await
}
#[tauri::command]
async fn chat_list_models() -> Result<Value, String> {
    let l = chat::list_models(None).await?;
    Ok(json!(l.iter().map(|id| json!({ "id": id, "price": chat::price_for(id).map(|p| json!({ "input": p.0, "cached": p.1, "output": p.2 })) })).collect::<Vec<_>>()))
}
#[tauri::command]
fn chat_get_usage() -> Value {
    chat::usage_summary()
}
#[tauri::command]
fn chat_reset_usage() -> Value {
    chat::reset_usage()
}
#[tauri::command]
fn chat_send(app: AppHandle, id: String, text: String, from: Option<String>) {
    chat::send(app, id, text, from);
}
#[tauri::command]
fn chat_stop(id: String) {
    chat::stop(&id);
}
#[tauri::command]
fn chat_new() {
    chat::new_chat();
}

// ------------------------------------------------------------ commandes : notes
#[tauri::command]
fn notes_load() -> Value {
    iris_store::read_json(&data_dir().join("notes.json")).unwrap_or(json!([]))
}
#[tauri::command]
fn notes_save(data: Value) -> Result<Value, String> {
    std::fs::write(data_dir().join("notes.json"), serde_json::to_string_pretty(&data).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true }))
}

// ------------------------------------------------------------ commandes : mini navigateur
#[tauri::command]
fn browser_get_pending() -> Option<String> {
    browser::take_pending()
}
// asynchrone : add_child attend le fil principal (une commande synchrone y tourne déjà -> blocage)
#[tauri::command]
async fn browser_view(app: AppHandle, url: Option<String>, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    browser::view(&app, url, x, y, w, h)
}
#[tauri::command]
fn browser_nav(app: AppHandle, action: String) {
    browser::nav(&app, &action);
}

fn main() {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| orb::recenter(app)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, sc, ev| {
                    if ev.state() != ShortcutState::Pressed {
                        return;
                    }
                    let s = sc.into_string().to_lowercase();
                    let id = if s.contains("space") { "chat" } else if s.ends_with('n') { "notes" } else { "browser" };
                    overlays::toggle(app, id);
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            orb_actions, orb_run_action, orb_set_ignore, orb_drag_start, orb_drag_end, orb_menu_open, orb_menu_close, orb_context_menu, orb_quit_ready,
            overlay_hide, overlay_set_opacity, overlay_save_opacity, overlay_get_opacity, chat_get_settings, chat_save_settings, chat_list_models, chat_get_usage, chat_reset_usage,
            chat_send, chat_stop, chat_new, notes_load, notes_save, browser_get_pending, browser_view, browser_nav
        ])
        .on_menu_event(|app, e| on_menu(app, e.id().as_ref()))
        .setup(|app| {
            let h = app.handle().clone();
            orb::refresh_monitors(&h);
            let (x, y) = orb::start_position(&h);
            let w = tauri::WebviewWindowBuilder::new(app, "orb", WebviewUrl::App("orb/index.html".into()))
                .title("Orbiz")
                .inner_size(orb::WIN, orb::WIN)
                .position(x, y)
                .transparent(true)
                .decorations(false)
                .shadow(false)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .focused(false)
                .build()?;
            let _ = w.set_ignore_cursor_events(true);
            orb::init(x, y, w.scale_factor().unwrap_or(1.0));
            let h2 = h.clone();
            w.on_window_event(move |e| {
                if let tauri::WindowEvent::Focused(false) = e {
                    let _ = h2.emit_to("orb", "force-close", ());
                }
            });
            orb::run(h.clone());

            for s in [CHAT_SHORTCUT, NOTES_SHORTCUT, BROWSER_SHORTCUT] {
                if let Err(e) = app.global_shortcut().register(s) {
                    eprintln!("[orbiz] raccourci {s} : {e}");
                }
            }
            let menu = build_menu(&h)?;
            tauri::tray::TrayIconBuilder::with_id("orbiz")
                .icon(tray_icon())
                .tooltip("Orbiz")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|t, e| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = e {
                        orb::recenter(t.app_handle());
                    }
                })
                .build(app)?;

            browser::listen_overlay(h.clone());
            // questions envoyées par les apps (iris-link) : on ouvre le chat et on lui transmet
            let h3 = h.clone();
            iris_link::host::listen(move |msg: Value| {
                let app = h3.clone();
                async move {
                    let existed = app.get_webview_window("chat").is_some();
                    overlays::open(&app, "chat");
                    if !existed {
                        tokio::time::sleep(Duration::from_millis(1500)).await;
                    }
                    let from = msg["from"]["name"].as_str().or(msg["from"]["id"].as_str()).map(String::from);
                    let _ = app.emit_to("chat", "chat:external", json!({ "text": msg["text"], "send": msg["send"].as_bool().unwrap_or(true), "from": from }));
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("démarrage d'Orbiz")
        .run(|_app, e| {
            if let tauri::RunEvent::ExitRequested { .. } = e {
                orb::save();
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn iris_apps_visibles() {
        let defs = iris_link::host::tool_defs();
        let apps: Vec<String> = iris_link::host::list_manifests().iter().filter_map(|m| m["id"].as_str().map(String::from)).collect();
        println!("apps: {apps:?} ; outils: {}", defs.len());
        assert!(defs.iter().all(|d| d["name"].as_str().unwrap().len() <= 64));
        assert!(iris_link::host::handles("app_click") && iris_link::host::handles("irisdj__get_state") && !iris_link::host::handles("navigate"));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let p = rt.block_on(iris_link::host::prompt_section());
        assert!(apps.is_empty() || p.contains("Apps de l'utilisateur"));
        let bt: Vec<serde_json::Value> = serde_json::from_str(include_str!("browser_tools.json")).unwrap();
        assert_eq!(bt.len(), 30);
    }
}
