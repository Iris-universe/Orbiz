// Navigateur : pilotage d'IrisBrowser (pipe `\\.\pipe\IrisBrowserSingleInstance`, une ligne JSON par commande)
// pour les outils du chat, et mini navigateur d'Orbiz (fenêtre `browser` + webview enfant, pipe `\\.\pipe\OrbizBrowserOverlay`).
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

const PIPE: &str = r"\\.\pipe\IrisBrowserSingleInstance";

/// Exécutable d'IrisBrowser (dans le dossier Iris).
pub fn iris_exe() -> std::path::PathBuf {
    crate::iris_root().join("IrisBrowser").join("dist").join("IrisBrowser").join("IrisBrowser.exe")
}
/// Ouvre une URL dans IrisBrowser (ou le navigateur par défaut s'il n'est pas installé).
pub fn open_url(url: &str) {
    let exe = iris_exe();
    if exe.exists() {
        let _ = std::process::Command::new(exe).arg(url).spawn();
    } else if url.starts_with("http://") || url.starts_with("https://") {
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
}

async fn send_command(cmd: &Value, timeout: Duration) -> Result<Value, String> {
    let fut = async {
        let c = ClientOptions::new().open(PIPE).map_err(|e| e.to_string())?;
        let (rd, mut wr) = tokio::io::split(c);
        let mut s = cmd.to_string();
        s.push('\n');
        wr.write_all(s.as_bytes()).await.map_err(|e| e.to_string())?;
        let mut lines = BufReader::new(rd).lines();
        match lines.next_line().await {
            Ok(Some(l)) => Ok(serde_json::from_str(&l).unwrap_or(json!({ "error": "Réponse invalide" }))),
            _ => Ok(json!({ "error": "Connexion fermée" })),
        }
    };
    tokio::time::timeout(timeout, fut).await.map_err(|_| "Timeout".to_string())?
}

/// Commandes lentes (analyse de scripts, rechargement + attente, plugins).
fn timeout_of(cmd: &str) -> Duration {
    Duration::from_secs(match cmd {
        "search_scripts" => 90,
        "list_scripts" | "get_script_source" | "live_edit_script" | "patch_script" | "create_plugin" => 45,
        "execute_js" => 40,
        "get_console" | "inspect_object" | "call_function_on" => 30,
        _ => 15,
    })
}

pub async fn send(cmd: Value) -> Value {
    let name = cmd["cmd"].as_str().unwrap_or("").to_string();
    let t = timeout_of(&name);
    match send_command(&cmd, t).await {
        Ok(v) => v,
        // le navigateur tourne mais il est lent : on ne relance pas la commande
        Err(e) if e == "Timeout" => json!({ "error": format!("Délai dépassé ({}s) pour {name}", t.as_secs()) }),
        Err(_) => {
            let _ = std::process::Command::new(iris_exe()).spawn();
            for _ in 0..8 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if let Ok(v) = send_command(&cmd, t).await {
                    return v;
                }
            }
            json!({ "error": "IrisBrowser ne répond pas. Lance-le d'abord." })
        }
    }
}

/// Outils du chat liés au navigateur (noms des outils -> commandes d'IrisBrowser).
pub async fn tool(name: &str, args: Value) -> Value {
    let a = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
    let with = |cmd: &str| {
        let mut m = args.as_object().cloned().unwrap_or_default();
        m.insert("cmd".into(), json!(cmd));
        Value::Object(m)
    };
    let cmd = match name {
        "navigate" => json!({ "cmd": "navigate", "url": a("url") }),
        "get_page_content" => json!({ "cmd": "get_page_content" }),
        "list_interactive_elements" => json!({ "cmd": "list_interactive" }),
        "click" => with("click"),
        "type_text" => json!({ "cmd": "type_text", "selector": a("selector"), "text": a("text"), "clear": a("clear") }),
        "select_option" => json!({ "cmd": "select_option", "selector": a("selector"), "value": a("value") }),
        "scroll" => json!({ "cmd": "scroll", "direction": if args["direction"].is_string() { a("direction") } else { json!("down") } }),
        "go_back" => json!({ "cmd": "go_back" }),
        "press_key" => json!({ "cmd": "press_key", "key": if args["key"].is_string() { a("key") } else { json!("Enter") } }),
        "show_browser" => json!({ "cmd": "show" }),
        "hide_browser" => json!({ "cmd": "hide" }),
        "execute_javascript" => {
            let mut m = args.as_object().cloned().unwrap_or_default();
            let code = m.remove("code").unwrap_or(json!(""));
            m.insert("cmd".into(), json!("execute_js"));
            m.insert("code".into(), code);
            Value::Object(m)
        }
        "modify_page" => with("modify_page"),
        "list_scripts" | "search_scripts" | "get_script_source" | "patch_script" | "live_edit_script" | "inspect_object" | "call_function_on" | "get_console"
        | "reload_page" | "list_modifications" | "remove_modification" | "get_plugin_api" | "get_internal_source" | "create_plugin" | "list_plugins"
        | "remove_plugin" => with(name),
        _ => return json!({ "error": format!("Outil inconnu : {name}") }),
    };
    send(cmd).await
}

// ------------------------------------------------------------ mini navigateur
static PENDING: Mutex<Option<String>> = Mutex::new(None);
const VIEW: &str = "browser-view";

/// Pipe où IrisBrowser envoie `{ cmd: "open", url }` pour ouvrir une page dans le mini navigateur.
pub fn listen_overlay(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let path = r"\\.\pipe\OrbizBrowserOverlay";
        let Ok(mut server) = ServerOptions::new().first_pipe_instance(true).create(path) else { return };
        loop {
            if server.connect().await.is_err() {
                continue;
            }
            let mut conn = server;
            let Ok(next) = ServerOptions::new().create(path) else { return };
            server = next;
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let mut buf = String::new();
                let _ = tokio::time::timeout(Duration::from_secs(5), conn.read_to_string(&mut buf)).await;
                if let Ok(m) = serde_json::from_str::<Value>(buf.trim()) {
                    if m["cmd"] == "open" {
                        if let Some(u) = m["url"].as_str() {
                            *PENDING.lock().unwrap() = Some(u.to_string());
                            crate::overlays::open(&app, "browser");
                            let _ = app.emit_to("browser", "browser:navigate", u);
                        }
                    }
                }
            });
        }
    });
}
pub fn take_pending() -> Option<String> {
    PENDING.lock().unwrap().take()
}

/// Affiche la page dans le cadre de la fenêtre (crée la webview au premier appel).
pub fn view(app: &AppHandle, url: Option<String>, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let rect = Rect { position: LogicalPosition::new(x, y).into(), size: LogicalSize::new(w.max(1.0), h.max(1.0)).into() };
    if let Some(v) = app.get_webview(VIEW) {
        let _ = v.set_bounds(rect);
        if let Some(u) = url.and_then(|u| u.parse().ok()) {
            let _ = v.navigate(u);
        }
        return Ok(());
    }
    let win = app.get_window("browser").ok_or("fenêtre fermée")?;
    let u: url::Url = url.as_deref().unwrap_or("about:blank").parse().map_err(|_| "URL invalide")?;
    let (a1, a2) = (app.clone(), app.clone());
    let b = tauri::webview::WebviewBuilder::new(VIEW, WebviewUrl::External(u))
        .data_directory(crate::data_dir().join("browser-profile"))
        .on_navigation(move |u| {
            let _ = a1.emit_to("browser", "browser:url", u.as_str());
            true
        })
        .on_document_title_changed(move |_, t| {
            let _ = a2.emit_to("browser", "browser:title", t);
        })
        .on_new_window(|u, _| {
            // les fenêtres popups s'ouvrent dans IrisBrowser
            open_url(u.as_str());
            tauri::webview::NewWindowResponse::Deny
        });
    win.add_child(b, LogicalPosition::new(x, y), LogicalSize::new(w.max(1.0), h.max(1.0))).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn nav(app: &AppHandle, action: &str) {
    if let Some(v) = app.get_webview(VIEW) {
        let _ = v.eval(if action == "back" { "history.back()" } else { "history.forward()" });
    }
}
