// Fenêtres flottantes d'Orbiz (chat, notes, mini navigateur) : transparentes, sans cadre, toujours au premier plan,
// redimensionnables. Fermer = cacher (la conversation reste jusqu'à la fermeture d'Orbiz). Position gardée dans overlays.json.
use serde_json::json;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow};

struct Def {
    id: &'static str,
    w: f64,
    h: f64,
    min_w: f64,
    min_h: f64,
}
const APPS: [Def; 3] = [
    Def { id: "chat", w: 440.0, h: 620.0, min_w: 320.0, min_h: 360.0 },
    Def { id: "notes", w: 380.0, h: 520.0, min_w: 300.0, min_h: 320.0 },
    Def { id: "browser", w: 520.0, h: 640.0, min_w: 380.0, min_h: 360.0 },
];

fn file() -> std::path::PathBuf {
    crate::data_dir().join("overlays.json")
}
static LAST_SAVE: Mutex<Option<Instant>> = Mutex::new(None);

fn save_bounds(w: &WebviewWindow) {
    // au plus toutes les 300 ms (déplacement continu)
    {
        let mut l = LAST_SAVE.lock().unwrap();
        if l.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return;
        }
        *l = Some(Instant::now());
    }
    let (Ok(p), Ok(z), Ok(s)) = (w.outer_position(), w.inner_size(), w.scale_factor()) else { return };
    let mut all = iris_store::read_json(&file()).unwrap_or(json!({}));
    all[w.label()] = json!({ "x": (p.x as f64 / s).round(), "y": (p.y as f64 / s).round(), "width": (z.width as f64 / s).round(), "height": (z.height as f64 / s).round() });
    let _ = iris_store::write_json(&file(), &all);
}

fn create(app: &AppHandle, d: &Def) -> Option<WebviewWindow> {
    let saved = iris_store::read_json(&file()).map(|a| a[d.id].clone()).filter(|v| v.is_object());
    let wa = app.primary_monitor().ok().flatten().map(|m| {
        let s = m.scale_factor();
        let a = m.work_area();
        (a.position.x as f64 / s, a.position.y as f64 / s, a.size.width as f64 / s, a.size.height as f64 / s)
    });
    let (x, y, w, h) = match (&saved, wa) {
        (Some(b), _) => (b["x"].as_f64().unwrap_or(100.0), b["y"].as_f64().unwrap_or(100.0), b["width"].as_f64().unwrap_or(d.w), b["height"].as_f64().unwrap_or(d.h)),
        (None, Some(a)) => (a.0 + a.2 - d.w - 120.0, a.1 + (a.3 - d.h) / 2.0, d.w, d.h),
        _ => (100.0, 100.0, d.w, d.h),
    };
    let win = tauri::WebviewWindowBuilder::new(app, d.id, WebviewUrl::App(format!("{}/index.html", d.id).into()))
        .title(match d.id {
            "chat" => "Orbiz Chat",
            "notes" => "Orbiz Notes",
            _ => "Iris Mini",
        })
        .inner_size(w, h)
        .position(x, y)
        .min_inner_size(d.min_w, d.min_h)
        // le mini navigateur n'est pas « transparent » au sens de Tauri (rendu DirectComposition, où la transparence
        // « layered » de Windows est sans effet) : sa transparence réglable est celle de toute la fenêtre
        .transparent(d.id != "browser")
        .background_color(tauri::window::Color(14, 11, 26, if d.id == "browser" { 255 } else { 0 }))
        .decorations(false)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .maximizable(false)
        .visible(false)
        .on_navigation(|u| {
            // les pages d'Orbiz (tauri.localhost) et about:blank passent ; un lien externe s'ouvre dans IrisBrowser
            let external = matches!(u.scheme(), "http" | "https") && !u.host_str().is_some_and(|h| h.ends_with("localhost"));
            if external {
                crate::browser::open_url(u.as_str());
            }
            !external
        })
        .on_new_window(|u, _| {
            // les liens s'ouvrent dans IrisBrowser, jamais dans la fenêtre
            if matches!(u.scheme(), "http" | "https") {
                crate::browser::open_url(u.as_str());
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .build()
        .ok()?;
    let w2 = win.clone();
    win.on_window_event(move |e| match e {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = w2.hide();
        }
        tauri::WindowEvent::Moved(_) => save_bounds(&w2),
        tauri::WindowEvent::Resized(_) => {
            save_bounds(&w2);
            reapply_opacity(&w2);
        }
        tauri::WindowEvent::Focused(_) => reapply_opacity(&w2),
        _ => {}
    });
    Some(win)
}

pub fn open(app: &AppHandle, id: &str) {
    let Some(d) = APPS.iter().find(|d| d.id == id) else { return };
    let w = match app.get_webview_window(id) {
        Some(w) => w,
        None => match create(app, d) {
            Some(w) => w,
            None => return,
        },
    };
    let _ = w.show();
    let _ = w.set_focus();
    reapply_opacity(&w);
    let _ = app.emit_to(id, "overlay:shown", ());
}

pub fn toggle(app: &AppHandle, id: &str) {
    if let Some(w) = app.get_webview_window(id) {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
            let _ = w.hide();
            return;
        }
    }
    open(app, id);
}

/// Transparence réglée pour une fenêtre (mémorisée dans overlays.json → `opacity`).
pub fn opacity_get(id: &str) -> Option<f64> {
    iris_store::read_json(&file()).and_then(|a| a["opacity"][id].as_f64())
}
pub fn opacity_save(id: &str, v: f64) {
    let mut all = iris_store::read_json(&file()).unwrap_or(json!({}));
    if !all["opacity"].is_object() {
        all["opacity"] = json!({});
    }
    all["opacity"][id] = json!(v);
    let _ = iris_store::write_json(&file(), &all);
}

/// Le mini navigateur affiche une autre webview : sa transparence est celle de toute la fenêtre (« layered »).
/// Tao recalcule les styles de la fenêtre (affichage, focus…) et retire WS_EX_LAYERED : on la réapplique après,
/// tout de suite puis un peu plus tard (le recalcul peut arriver après l'évènement).
pub fn reapply_opacity(w: &WebviewWindow) {
    if w.label() != "browser" {
        return;
    }
    let Some(v) = opacity_get("browser") else { return };
    set_opacity(w, v);
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        set_opacity(&w, v);
    });
}

/// Transparence de toute la fenêtre (comme `setOpacity` d'Electron : fenêtre « layered »).
pub fn set_opacity(w: &WebviewWindow, v: f64) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let Ok(h) = w.hwnd() else { return };
    let hwnd = h.0 as windows_sys::Win32::Foundation::HWND;
    let a = (v.clamp(0.15, 1.0) * 255.0).round() as u8;
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED as isize == 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        }
        SetLayeredWindowAttributes(hwnd, 0, a, LWA_ALPHA);
    }
}
