// Chat IA d'Orbiz (API OpenAI Responses, en streaming) : boucle agentique avec les outils du navigateur (IrisBrowser)
// et ceux des apps (iris-link). Réglages dans chat.json (clé chiffrée), dépenses estimées dans usage.json.
use crate::{browser, data_dir};
use futures::StreamExt;
use iris_link::host;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const API: &str = "https://api.openai.com/v1";
const SYSTEM_PROMPT: &str = include_str!("system_prompt.txt");

const MAX_TOOL_OUTPUT: usize = 6000; // caractères gardés pour un résultat d'outil récent
const OLD_TOOL_OUTPUT: usize = 300; // pour les résultats des messages précédents
const TURN_OUTPUT_BUDGET: usize = 24000; // total des résultats gardés en entier dans le message en cours
const KEEP_RECENT_OUTPUTS: usize = 3; // résultats les plus récents jamais raccourcis
const DEFAULT_COST_LIMIT: f64 = 0.15; // $ par message (réglable)
const MAX_ITERATIONS: usize = 60;

static CONVERSATION: Mutex<Vec<Value>> = Mutex::new(Vec::new());
static REQUESTS: Mutex<Option<HashMap<String, Arc<AtomicBool>>>> = Mutex::new(None);

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn emit(app: &AppHandle, ev: &str, v: Value) {
    let _ = app.emit_to("chat", ev, v);
}

// ------------------------------------------------------------ réglages
fn file() -> std::path::PathBuf {
    data_dir().join("chat.json")
}
fn load() -> Value {
    iris_store::read_json(&file()).unwrap_or(json!({}))
}
fn save(v: &Value) {
    let _ = iris_store::write_json(&file(), v);
}
/// Clé API (chiffrée DPAPI ; reprise automatique d'une clé safeStorage de l'ancienne version Electron).
fn get_key() -> Option<String> {
    let mut d = load();
    let raw = s(&d["key"]).to_string();
    if raw.is_empty() {
        return None;
    }
    let k = iris_secrets::unprotect_any(&raw, &data_dir());
    if k.is_empty() {
        return None;
    }
    if !raw.starts_with("dpapi:") {
        d["key"] = json!(iris_secrets::protect(&k));
        save(&d);
    }
    Some(k)
}
fn cost_limit(d: &Value) -> f64 {
    d["costLimit"].as_f64().unwrap_or(DEFAULT_COST_LIMIT)
}
fn settings_out(d: &Value) -> Value {
    json!({ "hasKey": get_key().is_some(), "model": d["model"], "opacity": d["opacity"].as_f64().unwrap_or(0.6), "costLimit": cost_limit(d) })
}
pub fn get_settings() -> Value {
    settings_out(&load())
}
pub async fn save_settings(p: Value) -> Result<Value, String> {
    let mut d = load();
    if let Some(key) = p["key"].as_str().filter(|k| !k.is_empty()) {
        let models = list_models(Some(key.to_string())).await?;
        d["key"] = json!(iris_secrets::protect(key));
        if d["model"].as_str().map(|m| !models.contains(&m.to_string())).unwrap_or(true) {
            d["model"] = json!(pick_default(&models));
        }
    }
    if let Some(m) = p["model"].as_str().filter(|m| !m.is_empty()) {
        d["model"] = json!(m);
    }
    if let Some(o) = p["opacity"].as_f64() {
        d["opacity"] = json!(o);
    }
    if let Some(c) = p["costLimit"].as_f64().filter(|c| c.is_finite() && *c >= 0.0) {
        d["costLimit"] = json!(c);
    }
    save(&d);
    Ok(settings_out(&d))
}

async fn api_error(r: reqwest::Response) -> String {
    let st = r.status().as_u16();
    r.json::<Value>().await.ok().and_then(|b| b["error"]["message"].as_str().map(String::from)).unwrap_or_else(|| format!("Erreur {st}"))
}
pub async fn list_models(key: Option<String>) -> Result<Vec<String>, String> {
    let key = key.or_else(get_key).ok_or("Aucune clé API configurée.")?;
    let r = reqwest::Client::new().get(format!("{API}/models")).bearer_auth(key).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(api_error(r).await);
    }
    let j: Value = r.json().await.map_err(|e| e.to_string())?;
    let keep = regex::Regex::new(r"^(gpt|o\d|chatgpt)").unwrap();
    let drop = regex::Regex::new(r"audio|realtime|tts|transcribe|image|search|embedding|moderation|instruct|codex").unwrap();
    let mut v: Vec<String> = j["data"].as_array().cloned().unwrap_or_default().iter().filter_map(|m| m["id"].as_str().map(String::from)).filter(|id| keep.is_match(id) && !drop.is_match(id)).collect();
    v.sort();
    v.reverse();
    Ok(v)
}
fn pick_default(models: &[String]) -> String {
    let dated = regex::Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();
    let plain: Vec<&String> = models.iter().filter(|m| !dated.is_match(m)).collect();
    let pool: Vec<&String> = if plain.is_empty() { models.iter().collect() } else { plain };
    pool.iter().find(|m| m.contains("mini")).or(pool.first()).map(|m| m.to_string()).unwrap_or_default()
}

// ------------------------------------------------------------ tarifs (dollars par million de tokens) et dépenses
// Source : page des tarifs OpenAI (relevé du 2026-11-05) ; OpenAI n'expose pas les prix par l'API.
// [entrée, entrée en cache (None = pas de remise), sortie]
const PRICES: &[(&str, f64, Option<f64>, f64)] = &[
    ("gpt-6-astra", 10.0, Some(1.0), 50.0), ("gpt-6.1-sol", 2.0, Some(0.1), 10.0), ("gpt-6.1-terra", 1.0, Some(0.05), 5.0),
    ("gpt-6-sol", 2.0, Some(0.2), 10.0), ("gpt-6-luna", 0.1, Some(0.01), 0.5), ("gpt-5.6-sol", 4.0, Some(0.4), 20.0),
    ("gpt-5.6-terra", 2.0, Some(0.2), 12.0), ("gpt-5.6-luna", 0.2, Some(0.02), 1.2), ("gpt-5.5", 5.0, Some(0.5), 30.0),
    ("gpt-5.5-pro", 30.0, None, 180.0), ("gpt-5.4", 2.5, Some(0.25), 15.0), ("gpt-5.4-pro", 30.0, None, 180.0),
    ("gpt-5.4-mini", 0.75, Some(0.075), 4.5), ("gpt-5.4-nano", 0.2, Some(0.02), 1.25), ("gpt-5.2", 1.75, Some(0.175), 14.0),
    ("gpt-5.2-pro", 21.0, None, 168.0), ("gpt-5.1", 1.25, Some(0.125), 10.0), ("gpt-5", 1.25, Some(0.125), 10.0),
    ("gpt-5-mini", 0.25, Some(0.025), 2.0), ("gpt-5-nano", 0.05, Some(0.005), 0.4), ("gpt-5-pro", 15.0, None, 120.0),
    ("gpt-4.1", 2.0, Some(0.5), 8.0), ("gpt-4.1-mini", 0.4, Some(0.1), 1.6), ("gpt-4.1-nano", 0.1, Some(0.025), 0.4),
    ("gpt-4o", 2.5, Some(1.25), 10.0), ("gpt-4o-mini", 0.15, Some(0.075), 0.6), ("o3-pro", 20.0, None, 80.0), ("o3", 2.0, Some(0.5), 8.0),
    ("o4-mini", 1.1, Some(0.275), 4.4), ("o3-mini", 1.1, Some(0.55), 4.4), ("o1", 15.0, Some(7.5), 60.0), ("o1-pro", 150.0, None, 600.0),
    ("chat-latest", 5.0, Some(0.5), 30.0), ("chatgpt-4o-latest", 5.0, None, 15.0), ("gpt-4-turbo", 10.0, None, 30.0),
    ("gpt-4", 30.0, None, 60.0), ("gpt-3.5-turbo", 0.5, None, 1.5),
];
/// Tarif d'un modèle : exact, sans suffixe de date, puis plus long préfixe « famille- ».
pub fn price_for(model: &str) -> Option<(f64, f64, f64)> {
    let id = model.to_lowercase();
    let shape = |p: &(&str, f64, Option<f64>, f64)| (p.1, p.2.unwrap_or(p.1), p.3);
    if let Some(p) = PRICES.iter().find(|p| p.0 == id) {
        return Some(shape(p));
    }
    let undated = regex::Regex::new(r"-\d{4}-\d{2}-\d{2}$").unwrap().replace(&id, "").to_string();
    if let Some(p) = PRICES.iter().find(|p| p.0 == undated) {
        return Some(shape(p));
    }
    PRICES.iter().filter(|p| undated.starts_with(&format!("{}-", p.0))).max_by_key(|p| p.0.len()).map(shape)
}
fn cost_of(model: &str, u: &Value) -> Option<f64> {
    let (pi, pc, po) = price_for(model)?;
    let input = u["input_tokens"].as_f64().unwrap_or(0.0);
    let cached = u["input_tokens_details"]["cached_tokens"].as_f64().unwrap_or(0.0).min(input);
    let output = u["output_tokens"].as_f64().unwrap_or(0.0);
    Some(((input - cached) * pi + cached * pc + output * po) / 1e6)
}
fn usage_file() -> std::path::PathBuf {
    data_dir().join("usage.json")
}
fn day_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
fn record_usage(model: &str, u: &Value) -> Option<f64> {
    let cost = cost_of(model, u);
    let mut d = iris_store::read_json(&usage_file()).unwrap_or(json!({ "days": {} }));
    if d["since"].is_null() {
        d["since"] = json!(chrono::Utc::now().to_rfc3339());
    }
    let m = &mut d["days"][day_key()][model];
    if m.is_null() {
        *m = json!({ "requests": 0, "input": 0, "cached": 0, "output": 0, "cost": 0.0, "unpriced": 0 });
    }
    let add = |m: &mut Value, k: &str, v: f64| m[k] = json!(m[k].as_f64().unwrap_or(0.0) + v);
    add(m, "requests", 1.0);
    add(m, "input", u["input_tokens"].as_f64().unwrap_or(0.0));
    add(m, "cached", u["input_tokens_details"]["cached_tokens"].as_f64().unwrap_or(0.0));
    add(m, "output", u["output_tokens"].as_f64().unwrap_or(0.0));
    match cost {
        Some(c) => add(m, "cost", c),
        None => add(m, "unpriced", 1.0),
    }
    let _ = iris_store::write_json(&usage_file(), &d);
    cost
}
pub fn usage_summary() -> Value {
    let d = iris_store::read_json(&usage_file()).unwrap_or(json!({ "days": {} }));
    let today = day_key();
    let month = today[..7].to_string();
    let (mut t, mut mo, mut tot, mut unpriced) = (0.0, 0.0, 0.0, 0.0);
    let mut by: Map<String, Value> = Map::new();
    for (day, models) in d["days"].as_object().cloned().unwrap_or_default() {
        for (model, m) in models.as_object().cloned().unwrap_or_default() {
            let c = m["cost"].as_f64().unwrap_or(0.0);
            tot += c;
            if day == today {
                t += c;
            }
            if day.starts_with(&month) {
                mo += c;
                let b = by.entry(model).or_insert(json!({ "cost": 0.0, "requests": 0.0, "tokens": 0.0, "unpriced": 0.0 }));
                let add = |b: &mut Value, k: &str, v: f64| b[k] = json!(b[k].as_f64().unwrap_or(0.0) + v);
                add(b, "cost", c);
                add(b, "requests", m["requests"].as_f64().unwrap_or(0.0));
                add(b, "unpriced", m["unpriced"].as_f64().unwrap_or(0.0));
                add(b, "tokens", m["input"].as_f64().unwrap_or(0.0) + m["output"].as_f64().unwrap_or(0.0));
                unpriced += m["unpriced"].as_f64().unwrap_or(0.0);
            }
        }
    }
    json!({ "today": t, "month": mo, "total": tot, "since": d["since"], "byModel": by, "unpriced": unpriced })
}
pub fn reset_usage() -> Value {
    let _ = iris_store::write_json(&usage_file(), &json!({ "days": {}, "since": chrono::Utc::now().to_rfc3339() }));
    usage_summary()
}

// ------------------------------------------------------------ journal des erreurs
fn log_error(model: &str, err: &str, extra: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("chat-errors.log")) {
        let _ = writeln!(f, "[{}] {model} {extra}\n  {err}", chrono::Utc::now().to_rfc3339());
    }
}

// ------------------------------------------------------------ outils
fn browser_tools() -> Vec<Value> {
    serde_json::from_str(include_str!("browser_tools.json")).unwrap_or_default()
}
async fn execute_tool(name: &str, args: Value) -> Value {
    match name {
        "wait" => {
            let secs = args["seconds"].as_f64().unwrap_or(1.0).clamp(0.1, 10.0);
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            json!({ "ok": true, "waited": secs })
        }
        n if host::handles(n) => host::execute(n, args).await,
        n => browser::tool(n, args).await,
    }
}

// ------------------------------------------------------------ taille du contexte
// Tout l'historique est renvoyé à chaque appel : sans ça, chaque résultat d'outil est refacturé à chaque étape.
fn clip(t: &str, max: usize) -> String {
    let n = t.chars().count();
    if n <= max {
        return t.to_string();
    }
    let head: String = t.chars().take(max).collect();
    format!("{head}\n[… tronqué : {} caractères de plus. Si besoin, redemande une portion plus petite et plus ciblée.]", n - max)
}
fn compact(conv: &mut [Value], turn_start: usize) {
    // 1. messages précédents : seulement un résumé des résultats et des gros arguments
    for it in conv.iter_mut().take(turn_start) {
        if it["type"] == "function_call_output" && s(&it["output"]).chars().count() > OLD_TOOL_OUTPUT + 120 {
            it["output"] = json!(clip(s(&it["output"]), OLD_TOOL_OUTPUT));
        } else if it["type"] == "function_call" && s(&it["arguments"]).len() > 1500 {
            it["arguments"] = json!(json!({ "_note": "arguments retirés de l'historique (trop longs)" }).to_string());
        }
    }
    // 2. message en cours : au-delà du budget, on raccourcit les plus anciens résultats
    let idx: Vec<usize> = (turn_start..conv.len()).filter(|&i| conv[i]["type"] == "function_call_output").collect();
    let mut total: usize = idx.iter().map(|&i| s(&conv[i]["output"]).chars().count()).sum();
    for &i in idx.iter().take(idx.len().saturating_sub(KEEP_RECENT_OUTPUTS)) {
        if total <= TURN_OUTPUT_BUDGET {
            break;
        }
        let len = s(&conv[i]["output"]).chars().count();
        if len > 800 {
            let c = clip(s(&conv[i]["output"]), 600);
            total = total - len + c.chars().count();
            conv[i]["output"] = json!(c);
        }
    }
}

// ------------------------------------------------------------ API (avec nouvelles tentatives)
struct Stop;
async fn call_api(key: &str, model: &str, input: Vec<Value>, instructions: &str, tools: &[Value], stop: &AtomicBool) -> Result<reqwest::Response, Result<String, Stop>> {
    let body = json!({ "model": model, "instructions": instructions, "input": input, "tools": tools, "stream": true });
    let client = reqwest::Client::new();
    for attempt in 0..=3u32 {
        if stop.load(Ordering::SeqCst) {
            return Err(Err(Stop));
        }
        let r = client.post(format!("{API}/responses")).bearer_auth(key).json(&body).send().await;
        let r = match r {
            Ok(r) => r,
            Err(e) => {
                // coupure réseau / connexion refusée : on réessaie un peu plus tard
                if attempt < 3 {
                    log_error(model, &e.to_string(), &format!("(réseau, nouvelle tentative {}/3)", attempt + 1));
                    tokio::time::sleep(Duration::from_millis(1500 * 2u64.pow(attempt))).await;
                    continue;
                }
                let m = format!("Impossible de joindre OpenAI ({e}). Vérifie ta connexion internet.");
                log_error(model, &m, "");
                return Err(Ok(m));
            }
        };
        if r.status().is_success() {
            return Ok(r);
        }
        let st = r.status().as_u16();
        let headers = r.headers().clone();
        let msg = api_error(r).await;
        if [500, 502, 503, 504].contains(&st) && attempt < 3 {
            log_error(model, &format!("Erreur API {st} : {msg}"), &format!("(nouvelle tentative {}/3)", attempt + 1));
            tokio::time::sleep(Duration::from_millis(2000 * 2u64.pow(attempt))).await;
            continue;
        }
        if st == 429 && attempt < 3 && !regex::Regex::new(r"(?i)quota|billing|too large").unwrap().is_match(&msg) {
            let h = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok());
            let ms = h("retry-after-ms").or(h("retry-after").map(|s| s * 1000.0)).unwrap_or(4000.0 * (attempt as f64 + 1.0));
            tokio::time::sleep(Duration::from_millis(ms.min(20000.0) as u64)).await;
            continue;
        }
        let m = if st >= 500 {
            format!("Erreur API {st} : OpenAI est momentanément indisponible (déjà réessayé 3 fois). Réessaie dans quelques minutes.")
        } else {
            format!("Erreur API {st} : {msg}")
        };
        log_error(model, &m, "");
        return Err(Ok(m));
    }
    Err(Ok("Erreur API".into()))
}

/// Lit le flux SSE : texte envoyé au fil de l'eau, éléments de sortie et consommation.
async fn parse_stream(app: &AppHandle, id: &str, r: reqwest::Response, stop: &AtomicBool) -> Result<(Vec<Value>, Option<Value>), String> {
    let mut buf = String::new();
    let mut items = vec![];
    let mut usage = None;
    let mut stream = r.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        buf.push_str(&String::from_utf8_lossy(&chunk.map_err(|e| e.to_string())?).replace("\r\n", "\n"));
        while let Some(i) = buf.find("\n\n") {
            let part: String = buf.drain(..i + 2).collect();
            let (mut ev, mut data) = (String::new(), vec![]);
            for line in part.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    ev = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
                }
            }
            let Ok(p) = serde_json::from_str::<Value>(&data.join("\n")) else { continue };
            match ev.as_str() {
                "response.output_text.delta" => {
                    if let Some(d) = p["delta"].as_str() {
                        emit(app, "chat:delta", json!({ "id": id, "text": d }));
                    }
                }
                "response.output_item.done" => {
                    if !p["item"].is_null() {
                        items.push(p["item"].clone());
                    }
                }
                "response.completed" | "response.incomplete" => {
                    if !p["response"]["usage"].is_null() {
                        usage = Some(p["response"]["usage"].clone());
                    }
                }
                "error" => return Err(p["message"].as_str().unwrap_or("Erreur de l'API").to_string()),
                _ => {}
            }
        }
    }
    Ok((items, usage))
}

// ------------------------------------------------------------ boucle agentique
pub fn send(app: AppHandle, id: String, text: String, from: Option<String>) {
    let content = match from.filter(|f| !f.is_empty()) {
        Some(f) => format!("[Depuis {f}] {text}"),
        None => text,
    };
    CONVERSATION.lock().unwrap().push(json!({ "role": "user", "content": content }));
    let stop = Arc::new(AtomicBool::new(false));
    REQUESTS.lock().unwrap().get_or_insert_with(HashMap::new).insert(id.clone(), stop.clone());
    tauri::async_runtime::spawn(async move {
        agent_loop(&app, &id, &stop).await;
        if let Some(m) = REQUESTS.lock().unwrap().as_mut() {
            m.remove(&id);
        }
    });
}
pub fn stop(id: &str) {
    if let Some(s) = REQUESTS.lock().unwrap().as_ref().and_then(|m| m.get(id)) {
        s.store(true, Ordering::SeqCst);
    }
}
pub fn new_chat() {
    CONVERSATION.lock().unwrap().clear();
}

async fn agent_loop(app: &AppHandle, id: &str, stop: &AtomicBool) {
    let Some(key) = get_key() else {
        return emit(app, "chat:error", json!({ "id": id, "message": "Ajoute ta clé API dans les réglages (⚙)." }));
    };
    let d = load();
    let model = s(&d["model"]).to_string();
    let limit = cost_limit(&d);
    let turn_start = CONVERSATION.lock().unwrap().len();
    let (mut turn_cost, mut unpriced) = (0.0, false);
    let instructions = format!("{SYSTEM_PROMPT}{}", host::prompt_section().await);
    let mut tools = browser_tools();
    tools.extend(host::tool_defs());

    let fail = |msg: String, cost: f64| {
        let mut c = CONVERSATION.lock().unwrap();
        let n = turn_start.saturating_sub(1);
        c.truncate(n);
        let spent = if cost > 0.0 { format!(" (déjà ≈ {cost:.3} $ dépensés pour ce message)") } else { String::new() };
        emit(app, "chat:error", json!({ "id": id, "message": format!("{msg}{spent}") }));
    };
    let aborted = || {
        CONVERSATION.lock().unwrap().truncate(turn_start);
        emit(app, "chat:done", json!({ "id": id }));
    };

    for it in 1..=MAX_ITERATIONS {
        if stop.load(Ordering::SeqCst) {
            return aborted();
        }
        let input = {
            let mut c = CONVERSATION.lock().unwrap();
            compact(&mut c, turn_start);
            c.clone()
        };
        let r = match call_api(&key, &model, input, &instructions, &tools, stop).await {
            Ok(r) => r,
            Err(Err(Stop)) => return aborted(),
            Err(Ok(m)) => return fail(m, turn_cost),
        };
        let (items, usage) = match parse_stream(app, id, r, stop).await {
            Ok(x) => x,
            Err(e) => {
                log_error(&model, &e, "");
                return fail(e, turn_cost);
            }
        };
        if stop.load(Ordering::SeqCst) {
            return aborted();
        }
        if let Some(u) = usage {
            match record_usage(&model, &u) {
                Some(c) => turn_cost += c,
                None => unpriced = true,
            }
        }
        let mut calls = vec![];
        {
            let mut c = CONVERSATION.lock().unwrap();
            for item in items {
                match s(&item["type"]) {
                    "message" => {
                        let text: String = item["content"].as_array().cloned().unwrap_or_default().iter().filter(|x| x["type"] == "output_text").map(|x| s(&x["text"]).to_string()).collect();
                        if !text.is_empty() {
                            c.push(json!({ "role": "assistant", "content": text }));
                        }
                    }
                    "function_call" => {
                        c.push(json!({ "type": "function_call", "call_id": item["call_id"], "name": item["name"], "arguments": item["arguments"] }));
                        calls.push(item);
                    }
                    _ => {}
                }
            }
        }
        if calls.is_empty() {
            break;
        }
        for fc in calls {
            let args: Value = serde_json::from_str(s(&fc["arguments"])).unwrap_or(json!({}));
            emit(app, "chat:status", json!({ "id": id, "tool": fc["name"], "args": args }));
            let res = execute_tool(s(&fc["name"]), args).await;
            let text = match res {
                Value::String(t) => t,
                v => v.to_string(),
            };
            CONVERSATION.lock().unwrap().push(json!({ "type": "function_call_output", "call_id": fc["call_id"], "output": clip(&text, MAX_TOOL_OUTPUT) }));
        }
        // garde-fou : on s'arrête avant de dépasser la limite de coût par message
        if limit > 0.0 && turn_cost >= limit {
            return emit(app, "chat:error", json!({ "id": id, "message": format!("Arrêt : ce message a déjà coûté environ {turn_cost:.3} $ (limite {limit} $). Dis « continue » pour poursuivre, ou change la limite dans les réglages.") }));
        }
        if it == MAX_ITERATIONS {
            return emit(app, "chat:error", json!({ "id": id, "message": format!("Arrêt après {MAX_ITERATIONS} étapes (≈ {turn_cost:.3} $). Dis « continue » pour poursuivre.") }));
        }
    }
    emit(app, "chat:done", json!({ "id": id, "cost": if unpriced { Value::Null } else { json!(turn_cost) } }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn prices() {
        assert_eq!(super::price_for("gpt-5-mini"), Some((0.25, 0.025, 2.0)));
        assert_eq!(super::price_for("gpt-5-mini-2026-01-01"), Some((0.25, 0.025, 2.0)));
        assert_eq!(super::price_for("gpt-5.4-pro"), Some((30.0, 30.0, 180.0)));
        assert_eq!(super::price_for("gpt-4o-audio-preview").map(|p| p.0), Some(2.5));
        assert!(super::price_for("inconnu").is_none());
        assert_eq!(super::clip("abcdef", 3).starts_with("abc\n"), true);
    }
}
