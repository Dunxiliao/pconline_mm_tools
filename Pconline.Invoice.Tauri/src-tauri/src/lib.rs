mod amazon;
mod db;
mod etailflow;
mod fedex;
mod invoice;
mod payment;
mod temu;
mod tracking;
mod tracking_db;
mod ups;
mod built_env;
use serde::{Deserialize, Serialize};

/// 读取 APP_VERSION（优先环境变量 / 内嵌配置；都没有则回退到 Tauri 包版本）
#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    get_config("APP_VERSION").unwrap_or_else(|| app.package_info().version.to_string())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthLoginRequest {
    account: String,
    password: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthRegisterRequest {
    account: String,
    password: String,
    display_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    remark: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthLoginResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthRegisterResponse {
    success: bool,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeConfigResponse {
    invoice_db_url: Option<String>,
    odoo_db_url: Option<String>,
    efship_db_url: Option<String>,
    wms_db_url: Option<String>,
    etailflow_url: Option<String>,
    merchant_code: Option<String>,
    env_name: Option<String>,
    tracking_upload: Option<bool>,
    invoice_upload: Option<bool>,
    payment_upload: Option<bool>,
}

fn auth_base_url() -> String {
    get_config("ETAILFLOW_URL").unwrap_or_else(|| "https://order.innovberg.com".to_string())
}

fn pick_str<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(v) = value.get(*key).and_then(|x| x.as_str()) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn pick_i64(value: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(v) = value.get(*key).and_then(|x| x.as_i64()) {
            return Some(v);
        }
    }
    None
}

fn parse_error_message(body: &str) -> Option<String> {
    if let Ok(raw) = serde_json::from_str::<serde_json::Value>(body) {
        let data = raw.get("data").unwrap_or(&raw);
        if let Some(msg) = pick_str(data, &["message", "msg"]) {
            return Some(msg.to_string());
        }
        if let Some(msg) = pick_str(&raw, &["message", "msg"]) {
            return Some(msg.to_string());
        }
    }
    None
}

fn parse_json_body(text: &str, scene: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(text).map_err(|e| format!("Parse {} response: {}", scene, e))
}

#[tauri::command]
async fn auth_login(payload: AuthLoginRequest) -> Result<AuthLoginResponse, String> {
    let url = format!("{}/api/auths/user-login", auth_base_url().trim_end_matches('/'));
    let merchant_code =
        get_config("ETAILFLOW_MERCHANT_CODE").unwrap_or_else(|| "pconline".to_string());
    let body = serde_json::json!({
        "merchant_code": merchant_code,
        "account": payload.account,
        "password": payload.password
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = parse_error_message(&text).unwrap_or_else(|| text.clone());
        let lower_msg = msg.to_lowercase();
        let friendly = if status.as_u16() == 401
            || status.as_u16() == 403
            || lower_msg.contains("invalid")
            || lower_msg.contains("incorrect")
            || lower_msg.contains("wrong password")
        {
            "Invalid username or password. Please try again.".to_string()
        } else if lower_msg.contains("not found") || lower_msg.contains("account does not exist") {
            "This account does not exist. Please register first.".to_string()
        } else {
            msg
        };
        return Err(friendly);
    }

    let raw: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Parse login response: {}", e))?;
    let data = raw.get("data").unwrap_or(&raw);
    let access_token = pick_str(data, &["access_token", "accessToken", "token"])
        .ok_or_else(|| "Login response missing access token".to_string())?
        .to_string();
    let refresh_token =
        pick_str(data, &["refresh_token", "refreshToken"]).map(|s| s.to_string());
    let expires_in = pick_i64(data, &["expires_in", "expiresIn"]).unwrap_or(0);

    Ok(AuthLoginResponse {
        access_token,
        refresh_token,
        expires_in,
    })
}

#[tauri::command]
async fn auth_register(payload: AuthRegisterRequest) -> Result<AuthRegisterResponse, String> {
    let url = format!("{}/api/auths/user-open", auth_base_url().trim_end_matches('/'));
    let merchant_code =
        get_config("ETAILFLOW_MERCHANT_CODE").unwrap_or_else(|| "pconline".to_string());
    let mut body = serde_json::Map::new();
    body.insert("merchant_code".to_string(), serde_json::Value::String(merchant_code));
    body.insert(
        "account".to_string(),
        serde_json::Value::String(payload.account.trim().to_string()),
    );
    body.insert(
        "password".to_string(),
        serde_json::Value::String(payload.password),
    );
    if let Some(v) = payload.display_name.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }) {
        body.insert("display_name".to_string(), serde_json::Value::String(v));
    }
    if let Some(v) = payload.email.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }) {
        body.insert("email".to_string(), serde_json::Value::String(v));
    }
    if let Some(v) = payload.phone.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }) {
        body.insert("phone".to_string(), serde_json::Value::String(v));
    }
    if let Some(v) = payload.remark.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }) {
        body.insert("remark".to_string(), serde_json::Value::String(v));
    }
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = parse_error_message(&text).unwrap_or_else(|| text.clone());
        let friendly = if status.as_u16() == 409
            && msg.to_lowercase().contains("account already exists")
        {
            "This account already exists. Please sign in directly or use another username."
                .to_string()
        } else {
            msg
        };
        return Err(friendly);
    }

    let raw: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Parse register response: {}", e))?;
    let data = raw.get("data").unwrap_or(&raw);
    let success = raw
        .get("success")
        .and_then(|v| v.as_bool())
        .or_else(|| data.get("success").and_then(|v| v.as_bool()))
        .unwrap_or(true);
    let message = pick_str(data, &["message", "msg"])
        .or_else(|| pick_str(&raw, &["message", "msg"]))
        .map(|s| s.to_string());

    Ok(AuthRegisterResponse { success, message })
}

#[tauri::command]
async fn auth_get_runtime_config(access_token: String) -> Result<RuntimeConfigResponse, String> {
    let url = format!("{}/api/configs/mm_tool_config", auth_base_url().trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("accept", "application/json")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Get runtime config failed ({}): {}", status.as_u16(), text));
    }

    let raw: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse runtime config response: {}", e))?;
    let cfg = raw.get("data").unwrap_or(&raw);

    Ok(RuntimeConfigResponse {
        invoice_db_url: pick_str(cfg, &["invoice_db_url", "invoiceDbUrl", "INVOICE_DB_URL"]).map(|s| s.to_string()),
        odoo_db_url: pick_str(cfg, &["odoo_db_url", "odooDbUrl", "ODOO_DB_URL"]).map(|s| s.to_string()),
        efship_db_url: pick_str(cfg, &["efship_db_url", "efshipDbUrl", "EFSHIP_DB_URL"]).map(|s| s.to_string()),
        wms_db_url: pick_str(cfg, &["wms_db_url", "wmsDbUrl", "WMS_DB_URL"]).map(|s| s.to_string()),
        etailflow_url: pick_str(cfg, &["etailflow_url", "etailflowUrl", "ETAILFLOW_URL"]).map(|s| s.to_string()),
        merchant_code: pick_str(cfg, &["merchant_code", "merchantCode", "ETAILFLOW_MERCHANT_CODE"]).map(|s| s.to_string()),
        env_name: pick_str(cfg, &["env_name", "envName", "APP_ENV"]).map(|s| s.to_string()),
        tracking_upload: cfg
            .get("tracking_upload")
            .and_then(|v| v.as_bool())
            .or_else(|| cfg.get("trackingUpload").and_then(|v| v.as_bool())),
        invoice_upload: cfg
            .get("invoice_upload")
            .and_then(|v| v.as_bool())
            .or_else(|| cfg.get("invoiceUpload").and_then(|v| v.as_bool())),
        payment_upload: cfg
            .get("payment_upload")
            .and_then(|v| v.as_bool())
            .or_else(|| cfg.get("paymentUpload").and_then(|v| v.as_bool())),
    })
}

#[tauri::command]
async fn auth_refresh(refresh_token: String) -> Result<AuthLoginResponse, String> {
    let url = format!("{}/api/auths/refresh", auth_base_url().trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("accept", "application/json")
        .bearer_auth(refresh_token.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Refresh token failed ({}): {}", status.as_u16(), text));
    }

    let raw: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Parse refresh response: {}", e))?;
    let data = raw.get("data").unwrap_or(&raw);
    let access_token = pick_str(data, &["access_token", "accessToken", "token"])
        .ok_or_else(|| "Refresh response missing access token".to_string())?
        .to_string();
    let new_refresh_token = pick_str(data, &["refresh_token", "refreshToken"])
        .map(|s| s.to_string())
        .or(Some(refresh_token));
    let expires_in = pick_i64(data, &["expires_in", "expiresIn"]).unwrap_or(0);

    Ok(AuthLoginResponse {
        access_token,
        refresh_token: new_refresh_token,
        expires_in,
    })
}

#[tauri::command]
async fn auth_list_users(
    access_token: String,
    page: i64,
    page_size: i64,
    merchant_code: String,
    user_name: Option<String>,
    is_active: Option<bool>,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/auths/users", auth_base_url().trim_end_matches('/'));
    let mut req = reqwest::Client::new()
        .get(url)
        .header("accept", "application/json")
        .bearer_auth(access_token)
        .query(&[
            ("page", page.to_string()),
            ("pageSize", page_size.to_string()),
            ("merchant_code", merchant_code),
        ]);
    if let Some(v) = user_name {
        if !v.trim().is_empty() {
            req = req.query(&[("user_name", v)]);
        }
    }
    if let Some(v) = is_active {
        req = req.query(&[("is_active", v.to_string())]);
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(parse_error_message(&text).unwrap_or(text));
    }
    let raw = parse_json_body(&text, "users list")?;
    Ok(raw.get("data").cloned().unwrap_or(raw))
}

#[tauri::command]
async fn auth_set_user_status(
    access_token: String,
    user_id: String,
    is_active: bool,
) -> Result<(), String> {
    let url = format!(
        "{}/api/auths/users/{}/status/{}",
        auth_base_url().trim_end_matches('/'),
        user_id,
        is_active
    );
    let resp = reqwest::Client::new()
        .post(url)
        .header("accept", "application/json")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(parse_error_message(&text).unwrap_or(text));
    }
    Ok(())
}

#[tauri::command]
async fn auth_reset_user_password(
    access_token: String,
    user_id: String,
    password: String,
) -> Result<(), String> {
    let url = format!(
        "{}/api/auths/user-reset-password",
        auth_base_url().trim_end_matches('/')
    );
    let body = serde_json::json!({
        "user_id": user_id,
        "password": password
    });
    let resp = reqwest::Client::new()
        .post(url)
        .header("accept", "application/json")
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(parse_error_message(&text).unwrap_or(text));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeConfigPayload {
    invoice_db_url: Option<String>,
    odoo_db_url: Option<String>,
    efship_db_url: Option<String>,
    wms_db_url: Option<String>,
    etailflow_url: Option<String>,
    merchant_code: Option<String>,
}

#[tauri::command]
fn apply_runtime_config(config: RuntimeConfigPayload) {
    if let Some(v) = config.invoice_db_url.filter(|s| !s.trim().is_empty()) {
        std::env::set_var("INVOICE_DB_URL", v);
    }
    if let Some(v) = config.odoo_db_url.filter(|s| !s.trim().is_empty()) {
        std::env::set_var("ODOO_DB_URL", v);
    }
    if let Some(v) = config.efship_db_url.filter(|s| !s.trim().is_empty()) {
        std::env::set_var("EFSHIP_DB_URL", v);
    }
    if let Some(v) = config.wms_db_url.filter(|s| !s.trim().is_empty()) {
        std::env::set_var("WMS_DB_URL", v);
    }
    if let Some(v) = config.etailflow_url.filter(|s| !s.trim().is_empty()) {
        std::env::set_var("ETAILFLOW_URL", v);
    }
    if let Some(v) = config.merchant_code.filter(|s| !s.trim().is_empty()) {
        std::env::set_var("ETAILFLOW_MERCHANT_CODE", v);
    }
}

/// 统一读取配置：优先环境变量，其次内嵌的 .env.prod 配置
pub fn get_config(key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    // release 构建时，使用 build.rs 生成的 built_env 作为兜底
    #[cfg(not(debug_assertions))]
    {
        if let Some(v) = crate::built_env::get(key) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn load_env_file(name: &str) {
    use std::path::{Path, PathBuf};

    fn try_load(path: &Path) -> bool {
        path.exists() && dotenvy::from_path_override(path).is_ok()
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    // 优先：项目根目录的 env（src-tauri 的上一级）
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest_dir = PathBuf::from(manifest_dir);
        if let Some(project_root) = manifest_dir.parent() {
            candidates.push(project_root.join("env").join(name));
        }
    }

    // 其次：当前工作目录及其上一级
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("env").join(name));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("env").join(name));
        }
    }

    // 最后：可执行文件目录及其上一级（打包运行）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("env").join(name));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("env").join(name));
            }
        }
    }

    for path in candidates {
        if try_load(&path) {
            return;
        }
    }
}

/// 从可执行文件所在目录或项目根加载 .env，支持按 APP_ENV 选择 .env.dev / .env.prod 等环境文件
fn load_dotenv() {
    // 1. 先加载通用 .env
    load_env_file(".env");

    // 2. 如果设置了 APP_ENV，则再加载对应的 .env.{APP_ENV}，用于覆盖通用配置
    if let Ok(env) = std::env::var("APP_ENV") {
        let env = env.trim();
        if !env.is_empty() {
            let filename = format!(".env.{}", env);
            load_env_file(&filename);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    load_dotenv();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_app_version,
            auth_login,
            auth_register,
            auth_get_runtime_config,
            auth_refresh,
            auth_list_users,
            auth_set_user_status,
            auth_reset_user_password,
            apply_runtime_config,
            invoice::invoice_process_files,
            invoice::invoice_check_files,
            invoice::invoice_upload_files,
            payment::payment_add_files,
            payment::payment_get_existing_customers,
            payment::payment_validate_file,
            payment::payment_process_file,
            tracking::tracking_parse_excel,
            tracking::tracking_post_etailflow,
            tracking::platform_list,
            tracking::get_etailflow_merchant_code,
            tracking::tracking_pull_database,
            tracking::tracking_assign_order,
            tracking::tracking_write_xlsx,
            tracking::tracking_download_template,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
