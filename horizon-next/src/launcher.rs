//! Minecraft install + launch logic (vanilla, offline-mode account).

use crate::auth::Session;
use crate::config::Instance;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Deserialize, Clone)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<VersionEntry>,
}

#[derive(Default, Clone)]
pub struct Progress {
    pub running: bool,
    pub message: String,
    pub fraction: f32,
    pub error: Option<String>,
}

pub type Shared = Arc<Mutex<Progress>>;

fn set(p: &Shared, msg: &str, fraction: f32) {
    let mut g = p.lock().unwrap();
    g.message = msg.to_string();
    g.fraction = fraction;
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("HorizonNext/0.1")
        .build()
        .map_err(|e| e.to_string())
}

pub fn fetch_versions() -> Result<Vec<VersionEntry>, String> {
    let m: Manifest = client()?
        .get(MANIFEST)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("version list: {e}"))?;
    Ok(m.versions)
}

fn get_json(c: &reqwest::blocking::Client, url: &str) -> Result<Value, String> {
    c.get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("{url}: {e}"))
}

fn download(c: &reqwest::blocking::Client, url: &str, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = c
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.bytes())
        .map_err(|e| format!("{url}: {e}"))?;
    let tmp = dest.with_extension("part");
    fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn rules_allow(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(|r| r.as_array()) else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let mut matches = true;
        if rule.get("features").is_some() {
            matches = false;
        }
        if let Some(os) = rule.get("os") {
            if let Some(n) = os.get("name").and_then(|n| n.as_str()) {
                if n != os_name() {
                    matches = false;
                }
            }
            if os.get("arch").is_some() || os.get("version").is_some() {
                matches = false;
            }
        }
        if matches {
            allowed = rule.get("action").and_then(|a| a.as_str()) == Some("allow");
        }
    }
    allowed
}

fn substitute(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

fn expand(arg: &Value, vars: &HashMap<&str, String>, out: &mut Vec<String>) {
    match arg {
        Value::String(s) => out.push(substitute(s, vars)),
        Value::Object(o) => {
            if !rules_allow(o.get("rules")) {
                return;
            }
            match o.get("value") {
                Some(Value::String(s)) => out.push(substitute(s, vars)),
                Some(Value::Array(a)) => {
                    for x in a {
                        if let Some(s) = x.as_str() {
                            out.push(substitute(s, vars));
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn extract_natives(jar: &Path, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = fs::File::open(jar).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() || f.name().starts_with("META-INF") {
            continue;
        }
        let Some(name) = f.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else {
            continue;
        };
        let mut out = fs::File::create(dir.join(name)).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn install_and_launch(
    root: &Path,
    inst: &Instance,
    session: &Session,
    java: &str,
    p: &Shared,
) -> Result<(), String> {
    let c = client()?;
    let libs_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    let game_dir = root.join("instances").join(&inst.name);
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;

    // 1. Version metadata
    set(p, "Fetching version metadata…", 0.02);
    let versions = fetch_versions()?;
    let entry = versions
        .iter()
        .find(|v| v.id == inst.version)
        .ok_or_else(|| format!("Unknown version {}", inst.version))?;
    let vjson = get_json(&c, &entry.url)?;
    let vdir = root.join("versions").join(&inst.version);
    fs::create_dir_all(&vdir).map_err(|e| e.to_string())?;
    let _ = fs::write(vdir.join(format!("{}.json", inst.version)), vjson.to_string());

    // 2. Client jar
    set(p, "Downloading game client…", 0.05);
    let client_url = vjson["downloads"]["client"]["url"]
        .as_str()
        .ok_or("No client download in version JSON")?;
    let client_jar = vdir.join(format!("{}.jar", inst.version));
    download(&c, client_url, &client_jar)?;

    // 3. Libraries
    let natives_dir = vdir.join("natives");
    let mut classpath: Vec<PathBuf> = Vec::new();
    let libs = vjson["libraries"].as_array().cloned().unwrap_or_default();
    for (i, lib) in libs.iter().enumerate() {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        set(
            p,
            &format!("Libraries {}/{}", i + 1, libs.len()),
            0.08 + 0.22 * (i as f32 / libs.len().max(1) as f32),
        );
        if let (Some(path), Some(url)) = (
            lib["downloads"]["artifact"]["path"].as_str(),
            lib["downloads"]["artifact"]["url"].as_str(),
        ) {
            let dest = libs_dir.join(path);
            download(&c, url, &dest)?;
            classpath.push(dest);
        }
        // Legacy natives (pre-1.19 style)
        if let Some(classifier) = lib["natives"][os_name()].as_str() {
            let key = classifier.replace("${arch}", "64");
            let cls = &lib["downloads"]["classifiers"][&key];
            if let (Some(path), Some(url)) = (cls["path"].as_str(), cls["url"].as_str()) {
                let dest = libs_dir.join(path);
                download(&c, url, &dest)?;
                extract_natives(&dest, &natives_dir)?;
            }
        }
    }
    classpath.push(client_jar);

    // 4. Assets (parallel)
    let index_id = vjson["assetIndex"]["id"].as_str().ok_or("No asset index")?.to_string();
    let index_url = vjson["assetIndex"]["url"].as_str().ok_or("No asset index url")?;
    let index_path = assets_dir.join("indexes").join(format!("{index_id}.json"));
    download(&c, index_url, &index_path)?;
    let idx: Value = serde_json::from_str(&fs::read_to_string(&index_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let jobs: Vec<String> = idx["objects"]
        .as_object()
        .map(|o| o.values().filter_map(|v| v["hash"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    let total = jobs.len().max(1);
    let queue = Arc::new(Mutex::new(jobs));
    let done = Arc::new(AtomicUsize::new(0));
    let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let (queue, done, failure, c, p) = (queue.clone(), done.clone(), failure.clone(), c.clone(), p.clone());
        let assets_dir = assets_dir.clone();
        handles.push(thread::spawn(move || loop {
            let hash = queue.lock().unwrap().pop();
            let Some(h) = hash else { break };
            let url = format!("https://resources.download.minecraft.net/{}/{}", &h[..2], h);
            let dest = assets_dir.join("objects").join(&h[..2]).join(&h);
            if let Err(e) = download(&c, &url, &dest) {
                *failure.lock().unwrap() = Some(e);
                break;
            }
            let n = done.fetch_add(1, Ordering::SeqCst) + 1;
            if n % 20 == 0 {
                set(&p, &format!("Assets {n}/{total}"), 0.30 + 0.60 * (n as f32 / total as f32));
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    if let Some(e) = failure.lock().unwrap().take() {
        return Err(e);
    }

    // 5. Build command line
    set(p, "Starting Minecraft…", 0.95);
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp = classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", session.name.clone());
    vars.insert("version_name", inst.version.clone());
    vars.insert("game_directory", game_dir.to_string_lossy().to_string());
    vars.insert("assets_root", assets_dir.to_string_lossy().to_string());
    vars.insert("assets_index_name", index_id);
    vars.insert("auth_uuid", session.uuid.clone());
    vars.insert("auth_access_token", session.access_token.clone());
    vars.insert("user_type", (if session.msa { "msa" } else { "legacy" }).to_string());
    vars.insert("version_type", "Horizon Next".into());
    vars.insert("natives_directory", natives_dir.to_string_lossy().to_string());
    vars.insert("launcher_name", "HorizonNext".into());
    vars.insert("launcher_version", env!("CARGO_PKG_VERSION").into());
    vars.insert("classpath", cp);
    vars.insert("clientid", "0".into());
    vars.insert("auth_xuid", "0".into());

    let mut jvm: Vec<String> = Vec::new();
    let mut game: Vec<String> = Vec::new();
    if let Some(args) = vjson.get("arguments") {
        for a in args["jvm"].as_array().into_iter().flatten() {
            expand(a, &vars, &mut jvm);
        }
        for a in args["game"].as_array().into_iter().flatten() {
            expand(a, &vars, &mut game);
        }
    } else {
        jvm.push(substitute("-Djava.library.path=${natives_directory}", &vars));
        jvm.push("-cp".into());
        jvm.push(substitute("${classpath}", &vars));
        if let Some(s) = vjson["minecraftArguments"].as_str() {
            game.extend(s.split_whitespace().map(|x| substitute(x, &vars)));
        }
    }
    let main_class = vjson["mainClass"].as_str().ok_or("No mainClass")?;

    Command::new(java)
        .arg(format!("-Xmx{}M", inst.ram_mb))
        .args(&jvm)
        .arg(main_class)
        .args(&game)
        .current_dir(&game_dir)
        .spawn()
        .map_err(|e| format!("Could not start Java ('{java}'): {e}"))?;

    set(p, "Game launched", 1.0);
    Ok(())
}
