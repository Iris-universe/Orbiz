// L'orbe : fenêtre carrée transparente toujours au premier plan, l'orbe est dessiné au centre.
// Physique (suivi du glisser, lancer, rebonds élastiques sur les bords, ressort) en coordonnées logiques (DIP), à 60 i/s.
// Les clics traversent la fenêtre sauf au-dessus de l'orbe : la position de la souris est envoyée à la page
// (évènement `cursor`), qui dit si elle survole une zone `.hit` (commande `orb_set_ignore`).
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

pub const WIN: f64 = 340.0;
const ORB_R: f64 = 32.0; // rayon de l'orbe fermé
const OPEN_R: f64 = 140.0; // rayon de l'orbe ouvert (pour rester entièrement visible)
const FPS: f64 = 60.0;
const FOLLOW: f64 = 0.42; // réactivité du suivi pendant le glisser
const FRICTION: f64 = 0.93; // glisse après un lancer
const WALL_K: f64 = 0.16; // raideur des bords de l'écran
const WALL_DAMP: f64 = 0.78;
const SPRING_K: f64 = 0.14; // ressort des déplacements automatiques
const SPRING_DAMP: f64 = 0.74;
const MAX_THROW: f64 = 70.0;

#[derive(Default)]
struct Drag {
    ox: f64,
    oy: f64,
    samples: VecDeque<(Instant, f64, f64)>,
}

#[derive(Default)]
pub struct Orb {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    drag: Option<Drag>,
    anchor: Option<(f64, f64)>,
    before_menu: Option<(f64, f64)>,
    pub menu_open: bool,
    running: bool,
    last_applied: (i32, i32),
    last_cursor: (i32, i32, bool),
    scale: f64,
}

pub static ORB: Mutex<Option<Orb>> = Mutex::new(None);

// ------------------------------------------------------------ écrans (coordonnées logiques)
#[derive(Clone, Copy)]
struct Mon {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    wa: (f64, f64, f64, f64),
    scale: f64,
}
static MONS: Mutex<Vec<Mon>> = Mutex::new(Vec::new());
/// Écrans (cache rafraîchi par la boucle, hors verrou de l'orbe : la requête passe par le fil principal).
fn monitors(_app: &AppHandle) -> Vec<Mon> {
    MONS.lock().unwrap().clone()
}
fn read_monitors(app: &AppHandle) -> Vec<Mon> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let s = m.scale_factor();
            let (p, z, wa) = (m.position(), m.size(), m.work_area());
            Mon {
                x: p.x as f64 / s,
                y: p.y as f64 / s,
                w: z.width as f64 / s,
                h: z.height as f64 / s,
                wa: (wa.position.x as f64 / s, wa.position.y as f64 / s, wa.size.width as f64 / s, wa.size.height as f64 / s),
                scale: s,
            }
        })
        .collect()
}
fn nearest(ms: &[Mon], x: f64, y: f64) -> Mon {
    let d = |m: &Mon| {
        let dx = if x < m.x { m.x - x } else if x > m.x + m.w { x - m.x - m.w } else { 0.0 };
        let dy = if y < m.y { m.y - y } else if y > m.y + m.h { y - m.y - m.h } else { 0.0 };
        dx * dx + dy * dy
    };
    ms.iter().copied().min_by(|a, b| d(a).partial_cmp(&d(b)).unwrap()).unwrap_or(Mon { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0, wa: (0.0, 0.0, 1920.0, 1040.0), scale: 1.0 })
}
/// Écran principal = celui qui contient (0, 0).
fn primary(app: &AppHandle) -> Mon {
    nearest(&monitors(app), 1.0, 1.0)
}
/// Curseur en coordonnées logiques (GetCursorPos : lisible depuis n'importe quel fil).
fn cursor(app: &AppHandle) -> (f64, f64) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    let (px, py) = (p.x as f64, p.y as f64);
    // l'écran qui contient le point physique
    let s = monitors(app)
        .iter()
        .find(|m| px >= m.x * m.scale && py >= m.y * m.scale && px < (m.x + m.w) * m.scale && py < (m.y + m.h) * m.scale)
        .map(|m| m.scale)
        .unwrap_or(1.0);
    (px / s, py / s)
}

struct Lim {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}
/// Bornes du coin haut-gauche pour que l'orbe (rayon r) reste dans la zone de travail.
fn limits_for(app: &AppHandle, px: f64, py: f64, r: f64) -> Lim {
    let wa = nearest(&monitors(app), px, py).wa;
    Lim { min_x: wa.0 - WIN / 2.0 + r, max_x: wa.0 + wa.2 - WIN / 2.0 - r, min_y: wa.1 - WIN / 2.0 + r, max_y: wa.1 + wa.3 - WIN / 2.0 - r }
}
fn clamp(v: f64, a: f64, b: f64) -> f64 {
    v.max(a).min(b)
}
/// Effet élastique quand on tire l'orbe au-delà du bord.
fn rubber(v: f64, min: f64, max: f64) -> f64 {
    if v < min {
        min - (min - v) * 0.3
    } else if v > max {
        max + (v - max) * 0.3
    } else {
        v
    }
}
pub fn refresh_monitors(app: &AppHandle) {
    let m = read_monitors(app);
    if !m.is_empty() {
        *MONS.lock().unwrap() = m;
    }
}
pub fn default_position(app: &AppHandle) -> (f64, f64) {
    let wa = primary(app).wa;
    (wa.0 + wa.2 - WIN / 2.0 - 80.0, wa.1 + wa.3 - WIN / 2.0 - 140.0)
}

// ------------------------------------------------------------ état persistant
fn state_file() -> std::path::PathBuf {
    crate::data_dir().join("state.json")
}
fn save_state(o: &Orb) {
    let _ = iris_store::write_json(&state_file(), &json!({ "x": o.x.round(), "y": o.y.round() }));
}
/// Position de départ (sauvegardée, recollée sur un écran existant).
pub fn start_position(app: &AppHandle) -> (f64, f64) {
    let s = iris_store::read_json(&state_file()).unwrap_or(json!({}));
    let (x, y) = match (s["x"].as_f64(), s["y"].as_f64()) {
        (Some(x), Some(y)) => (x, y),
        _ => default_position(app),
    };
    let l = limits_for(app, x + WIN / 2.0, y + WIN / 2.0, ORB_R);
    (clamp(x, l.min_x, l.max_x), clamp(y, l.min_y, l.max_y))
}

pub fn init(x: f64, y: f64, scale: f64) {
    *ORB.lock().unwrap() = Some(Orb { x, y, scale, last_applied: (i32::MIN, i32::MIN), ..Default::default() });
}

fn apply(app: &AppHandle, o: &mut Orb) {
    let (x, y) = (o.x.round() as i32, o.y.round() as i32);
    if (x, y) == o.last_applied {
        return;
    }
    o.last_applied = (x, y);
    if let Some(w) = app.get_webview_window("orb") {
        let s = nearest(&monitors(app), o.x + WIN / 2.0, o.y + WIN / 2.0).scale;
        let _ = w.set_position(PhysicalPosition::new((o.x * s).round() as i32, (o.y * s).round() as i32));
        if (o.scale - s).abs() > 0.01 {
            // la fenêtre change d'écran : on garde sa taille logique
            o.scale = s;
            let _ = w.set_size(tauri::PhysicalSize::new((WIN * s).round() as u32, (WIN * s).round() as u32));
        }
    }
}
fn send_motion(app: &AppHandle, o: &Orb) {
    let _ = app.emit_to("orb", "motion", json!({ "vx": o.vx, "vy": o.vy }));
}

/// Boucle de l'orbe (60 i/s) : suivi de la souris pour la page + physique quand elle est active.
pub fn run(app: AppHandle) {
    std::thread::spawn(move || {
      let mut last_mons = Instant::now();
      loop {
        std::thread::sleep(Duration::from_secs_f64(1.0 / FPS));
        if last_mons.elapsed() > Duration::from_secs(2) {
            last_mons = Instant::now();
            refresh_monitors(&app);
        }
        let (cx, cy) = cursor(&app);
        let mut g = ORB.lock().unwrap();
        let Some(o) = g.as_mut() else { continue };
        // position de la souris dans la fenêtre (la page en a besoin même quand les clics la traversent)
        let (lx, ly) = (cx - o.x, cy - o.y);
        let inside = lx >= 0.0 && ly >= 0.0 && lx < WIN && ly < WIN;
        let key = (lx.round() as i32, ly.round() as i32, inside);
        if key != o.last_cursor {
            o.last_cursor = key;
            let _ = app.emit_to("orb", "cursor", json!({ "x": lx, "y": ly, "inside": inside }));
        }
        if o.running {
            tick(&app, o, cx, cy);
        }
      }
    });
}

fn stop(app: &AppHandle, o: &mut Orb) {
    o.running = false;
    o.vx = 0.0;
    o.vy = 0.0;
    send_motion(app, o);
    save_state(o);
}

fn tick(app: &AppHandle, o: &mut Orb, cx: f64, cy: f64) {
    if let Some(d) = o.drag.as_mut() {
        let now = Instant::now();
        d.samples.push_back((now, cx, cy));
        while d.samples.len() > 2 && now.duration_since(d.samples[0].0) > Duration::from_millis(90) {
            d.samples.pop_front();
        }
        let l = limits_for(app, cx, cy, ORB_R);
        let tx = rubber(cx - d.ox, l.min_x, l.max_x);
        let ty = rubber(cy - d.oy, l.min_y, l.max_y);
        let nx = o.x + (tx - o.x) * FOLLOW;
        let ny = o.y + (ty - o.y) * FOLLOW;
        o.vx = nx - o.x;
        o.vy = ny - o.y;
        o.x = nx;
        o.y = ny;
    } else if let Some((ax, ay)) = o.anchor {
        o.vx = (o.vx + (ax - o.x) * SPRING_K) * SPRING_DAMP;
        o.vy = (o.vy + (ay - o.y) * SPRING_K) * SPRING_DAMP;
        o.x += o.vx;
        o.y += o.vy;
        if (ax - o.x).hypot(ay - o.y) < 0.5 && o.vx.hypot(o.vy) < 0.3 {
            o.x = ax;
            o.y = ay;
            o.anchor = None;
            apply(app, o);
            return stop(app, o);
        }
    } else {
        // glisse libre avec rebonds élastiques sur les bords
        let r = if o.menu_open { OPEN_R } else { ORB_R };
        let l = limits_for(app, o.x + WIN / 2.0, o.y + WIN / 2.0, r);
        o.vx *= FRICTION;
        o.vy *= FRICTION;
        let mut outside = false;
        if o.x < l.min_x || o.x > l.max_x {
            o.vx = (o.vx + (clamp(o.x, l.min_x, l.max_x) - o.x) * WALL_K) * WALL_DAMP;
            outside = true;
        }
        if o.y < l.min_y || o.y > l.max_y {
            o.vy = (o.vy + (clamp(o.y, l.min_y, l.max_y) - o.y) * WALL_K) * WALL_DAMP;
            outside = true;
        }
        o.x += o.vx;
        o.y += o.vy;
        if o.vx.hypot(o.vy) < 0.08 && !outside {
            apply(app, o);
            return stop(app, o);
        }
    }
    apply(app, o);
    send_motion(app, o);
}

fn with<R>(f: impl FnOnce(&mut Orb) -> R) -> Option<R> {
    ORB.lock().unwrap().as_mut().map(f)
}

pub fn recenter(app: &AppHandle) {
    let p = default_position(app);
    with(|o| {
        o.anchor = Some(p);
        o.running = true;
    });
}

pub fn drag_start(app: &AppHandle) {
    let (cx, cy) = cursor(app);
    with(|o| {
        o.anchor = None;
        o.before_menu = None;
        o.drag = Some(Drag { ox: cx - o.x, oy: cy - o.y, samples: VecDeque::from([(Instant::now(), cx, cy)]) });
        o.running = true;
    });
}

pub fn drag_end() {
    with(|o| {
        let Some(d) = o.drag.take() else { return };
        if let (Some(f), Some(l)) = (d.samples.front(), d.samples.back()) {
            let dt = l.0.duration_since(f.0).as_secs_f64() * 1000.0;
            if dt > 0.0 && l.0.elapsed() < Duration::from_millis(60) {
                let k = 1000.0 / FPS / dt;
                o.vx = clamp((l.1 - f.1) * k, -MAX_THROW, MAX_THROW);
                o.vy = clamp((l.2 - f.2) * k, -MAX_THROW, MAX_THROW);
            }
        }
        o.running = true;
    });
}

pub fn menu_open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("orb") {
        let _ = w.set_focus();
    }
    with(|o| {
        o.menu_open = true;
        // si l'orbe ouvert dépasserait de l'écran, on le décale doucement
        let l = limits_for(app, o.x + WIN / 2.0, o.y + WIN / 2.0, OPEN_R);
        let t = (clamp(o.x, l.min_x, l.max_x), clamp(o.y, l.min_y, l.max_y));
        if (t.0 - o.x).abs() > 1.0 || (t.1 - o.y).abs() > 1.0 {
            o.before_menu = Some((o.x, o.y));
            o.anchor = Some(t);
            o.running = true;
        }
    });
}

pub fn menu_close() {
    with(|o| {
        o.menu_open = false;
        if let Some(b) = o.before_menu.take() {
            o.anchor = Some(b);
            o.running = true;
        }
    });
}

pub fn save() {
    with(|o| save_state(o));
}
