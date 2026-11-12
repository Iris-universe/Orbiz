// Boutons affichés quand l'orbe s'ouvre. La liste est dans %APPDATA%\orbiz\actions.json (créée au premier lancement
// avec les boutons par défaut ci-dessous) : on y ajoute ses propres raccourcis, ils ne sont jamais dans le code.
//
// Types (`kind`) :
//   overlay   { overlay: "chat" | "notes" | "browser" }        fenêtre d'Orbiz
//   iris      { app: "<id iris-link>", exe: "Dossier/chemin.exe" } ou { app, electron: "Dossier" }  app du dossier Iris
//   irisbrowser                                                  IrisBrowser
//   exe       { path, args? }                                    programme
//   open      { path }  (chemin absolu ou relatif au menu Démarrer commun)   raccourci / fichier
// icon = clé d'une icône de ui/orb/icons.js.
use iris_link::host;
use serde_json::{json, Value};
use tauri::AppHandle;

fn defaults() -> Value {
    json!([
        { "id": "iris", "label": "Iris Browser", "icon": "globe", "kind": "irisbrowser" },
        { "id": "chatgpt", "label": "ChatGPT", "icon": "chat", "kind": "overlay", "overlay": "chat" },
        { "id": "notes", "label": "Notes", "icon": "notes", "kind": "overlay", "overlay": "notes" },
        { "id": "browser", "label": "Mini Browser", "icon": "browser", "kind": "overlay", "overlay": "browser" },
        { "id": "revision", "label": "Révision", "icon": "book", "kind": "iris", "app": "revision", "electron": "revision" },
        { "id": "irisdj", "label": "IrisDJ", "icon": "disc", "kind": "iris", "app": "irisdj", "exe": "IrisDJ/target/release/irisdj.exe" },
        { "id": "irisstudio", "label": "IrisStudio", "icon": "steps", "kind": "iris", "app": "irisstudio", "exe": "IrisStudio/target/release/irisstudio.exe" },
        { "id": "irisprog", "label": "IrisProg", "icon": "whale", "kind": "iris", "app": "irisprog", "exe": "IrisProg/target/release/irisprog.exe" },
        { "id": "calc", "label": "Calculatrice", "icon": "calc", "kind": "exe", "path": "calc.exe" },
    ])
}

fn file() -> std::path::PathBuf {
    crate::data_dir().join("actions.json")
}

pub fn list() -> Vec<Value> {
    let f = file();
    let v = iris_store::read_json(&f).filter(|v| v.is_array()).unwrap_or_else(|| {
        let d = defaults();
        let _ = iris_store::write_json(&f, &d);
        d
    });
    v.as_array().cloned().unwrap_or_default()
}

/// Pour la page : id, label, icon.
pub fn public_list() -> Value {
    json!(list().iter().map(|a| json!({ "id": a["id"], "label": a["label"], "icon": a["icon"] })).collect::<Vec<_>>())
}

pub fn run(app: &AppHandle, id: &str) {
    let Some(a) = list().into_iter().find(|a| a["id"].as_str() == Some(id)) else { return };
    let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
    match s("kind").as_str() {
        "overlay" => crate::overlays::open(app, &s("overlay")),
        "irisbrowser" => {
            let _ = std::process::Command::new(crate::browser::iris_exe()).spawn();
        }
        "exe" => {
            let args: Vec<String> = a["args"].as_array().map(|l| l.iter().filter_map(|x| x.as_str()).map(String::from).collect()).unwrap_or_default();
            let _ = std::process::Command::new(s("path")).args(args).spawn();
        }
        "open" => {
            let p = std::path::PathBuf::from(s("path"));
            let p = if p.is_absolute() { p } else { std::path::PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs").join(p) };
            iris_store::open_path(&p);
        }
        "iris" => {
            let (app_id, root) = (s("app"), crate::iris_root());
            let launch = if !s("exe").is_empty() {
                let exe = root.join(s("exe"));
                let dir = root.join(s("exe").split('/').next().unwrap_or(""));
                json!({ "command": exe.to_string_lossy(), "args": [], "cwd": dir.to_string_lossy() })
            } else {
                let dir = root.join(s("electron"));
                let exe = dir.join("node_modules").join("electron").join("dist").join("electron.exe");
                json!({ "command": exe.to_string_lossy(), "args": [dir.to_string_lossy()], "cwd": dir.to_string_lossy() })
            };
            // remet la fenêtre devant si elle est ouverte, sinon la lance
            tauri::async_runtime::spawn(async move {
                if host::is_running(&app_id).await {
                    host::open(&app_id).await;
                } else {
                    host::launch(&launch);
                }
            });
        }
        _ => {}
    }
}
