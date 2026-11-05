//! Microsoft → Xbox Live → XSTS → Minecraft authentication (device-code flow).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const TENANT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Clone)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub msa: bool,
}

impl Session {
    /// Offline-mode session (UUID is stable per name, not Mojang's exact algorithm).
    pub fn offline(name: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut a = std::collections::hash_map::DefaultHasher::new();
        format!("OfflinePlayer:{name}").hash(&mut a);
        let mut b = std::collections::hash_map::DefaultHasher::new();
        format!("Horizon:{name}").hash(&mut b);
        Session {
            name: name.to_string(),
            uuid: format!("{:016x}{:016x}", a.finish(), b.finish()),
            access_token: "0".into(),
            msa: false,
        }
    }
}

/// Persisted account (refresh token lets us sign in again without a browser).
#[derive(Serialize, Deserialize, Clone)]
pub struct Stored {
    pub refresh_token: String,
    pub name: String,
    pub uuid: String,
}

#[derive(Default, Clone)]
pub enum LoginState {
    #[default]
    Idle,
    Code { code: String, url: String },
    Working,
    Done,
    Failed(String),
}

fn file(root: &Path) -> PathBuf {
    root.join("account.json")
}

pub fn load(root: &Path) -> Option<Stored> {
    serde_json::from_str(&fs::read_to_string(file(root)).ok()?).ok()
}

fn save(root: &Path, s: &Stored) {
    let _ = fs::create_dir_all(root);
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = fs::write(file(root), t);
    }
}

pub fn clear(root: &Path) {
    let _ = fs::remove_file(file(root));
}

fn http() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("HorizonNext/0.1")
        .build()
        .map_err(|e| e.to_string())
}

fn post_json(c: &reqwest::blocking::Client, url: &str, body: &Value) -> Result<Value, String> {
    let r = c
        .post(url)
        .header("Accept", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("{url}: {e}"))?;
    let status = r.status();
    let v: Value = r.json().map_err(|e| format!("{url}: {e}"))?;
    if !status.is_success() {
        return Err(format!("{url}: {status} {v}"));
    }
    Ok(v)
}

fn token_request(c: &reqwest::blocking::Client, params: &[(&str, &str)]) -> Result<Value, String> {
    c.post(format!("{TENANT}/token"))
        .form(params)
        .send()
        .and_then(|r| r.json())
        .map_err(|e| format!("Microsoft token request: {e}"))
}

fn friendly_xsts(e: String) -> String {
    if e.contains("2148916233") {
        "This Microsoft account has no Xbox profile. Create one at xbox.com, then try again.".into()
    } else if e.contains("2148916238") {
        "This is a child account: an adult must add it to a Microsoft Family first.".into()
    } else if e.contains("2148916235") {
        "Xbox Live is not available in your country/region.".into()
    } else {
        e
    }
}

/// Microsoft access token → Minecraft session.
fn minecraft_session(c: &reqwest::blocking::Client, ms_token: &str) -> Result<Session, String> {
    let xbl = post_json(
        c,
        "https://user.auth.xboxlive.com/user/authenticate",
        &json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
    )?;
    let xbl_token = xbl["Token"].as_str().ok_or("Xbox Live returned no token")?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or("Xbox Live returned no user hash")?;

    let xsts = post_json(
        c,
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        &json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
    )
    .map_err(friendly_xsts)?;
    let xsts_token = xsts["Token"].as_str().ok_or("XSTS returned no token")?;

    let mc = post_json(
        c,
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        &json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }),
    )?;
    let access = mc["access_token"].as_str().ok_or("Minecraft returned no access token")?.to_string();

    let r = c
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&access)
        .send()
        .map_err(|e| e.to_string())?;
    if r.status().as_u16() == 404 {
        return Err("This account does not own Minecraft: Java Edition.".into());
    }
    let prof: Value = r.json().map_err(|e| e.to_string())?;
    Ok(Session {
        name: prof["name"].as_str().ok_or("No profile name")?.to_string(),
        uuid: prof["id"].as_str().ok_or("No profile id")?.to_string(),
        access_token: access,
        msa: true,
    })
}

fn login_device(root: &Path, client_id: &str, state: &Arc<Mutex<LoginState>>) -> Result<(), String> {
    if client_id.trim().is_empty() {
        return Err("Enter your Azure application (client) ID in Settings first.".into());
    }
    let c = http()?;
    let d: Value = c
        .post(format!("{TENANT}/devicecode"))
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .and_then(|r| r.json())
        .map_err(|e| e.to_string())?;
    let device = d["device_code"]
        .as_str()
        .ok_or_else(|| format!("Microsoft: {}", d["error_description"].as_str().unwrap_or("device code request failed")))?
        .to_string();
    *state.lock().unwrap() = LoginState::Code {
        code: d["user_code"].as_str().unwrap_or("").to_string(),
        url: d["verification_uri"].as_str().unwrap_or("https://microsoft.com/link").to_string(),
    };
    let mut interval = d["interval"].as_u64().unwrap_or(5);
    let deadline = d["expires_in"].as_u64().unwrap_or(900);
    let mut waited = 0;
    let tokens = loop {
        thread::sleep(Duration::from_secs(interval));
        waited += interval;
        if waited > deadline {
            return Err("The sign-in code expired. Please try again.".into());
        }
        let v = token_request(
            &c,
            &[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", &device),
            ],
        )?;
        match v["error"].as_str() {
            None => break v,
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some(e) => return Err(format!("Sign-in failed: {e}")),
        }
    };
    *state.lock().unwrap() = LoginState::Working;
    let access = tokens["access_token"].as_str().ok_or("No Microsoft access token")?;
    let refresh = tokens["refresh_token"].as_str().ok_or("No refresh token")?;
    let s = minecraft_session(&c, access)?;
    save(root, &Stored { refresh_token: refresh.into(), name: s.name, uuid: s.uuid });
    Ok(())
}

pub fn start_login(root: PathBuf, client_id: String, state: Arc<Mutex<LoginState>>) {
    *state.lock().unwrap() = LoginState::Working;
    thread::spawn(move || {
        let r = login_device(&root, &client_id, &state);
        *state.lock().unwrap() = match r {
            Ok(()) => LoginState::Done,
            Err(e) => LoginState::Failed(e),
        };
    });
}

/// Use the stored refresh token to get a fresh Minecraft session.
pub fn refresh_session(root: &Path, client_id: &str) -> Result<Session, String> {
    let stored = load(root).ok_or("Not signed in")?;
    let c = http()?;
    let v = token_request(
        &c,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", &stored.refresh_token),
            ("scope", SCOPE),
        ],
    )?;
    let access = v["access_token"]
        .as_str()
        .ok_or("Your Microsoft session expired. Sign in again in Settings.")?;
    let s = minecraft_session(&c, access)?;
    save(
        root,
        &Stored {
            refresh_token: v["refresh_token"].as_str().unwrap_or(&stored.refresh_token).to_string(),
            name: s.name.clone(),
            uuid: s.uuid.clone(),
        },
    );
    Ok(s)
}
