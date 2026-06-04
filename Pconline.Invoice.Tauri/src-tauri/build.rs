use std::{env, fs, path::PathBuf};

fn main() {
    // 正常的 Tauri 代码生成
    tauri_build::build();

    // 从项目根目录下的 env/.env.{APP_ENV} 读取配置，生成内嵌常量（仅用于 release 作为兜底）
    // - APP_ENV=prod -> env/.env.prod
    // - APP_ENV=test -> env/.env.test
    // - 未设置则默认 prod
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let project_root = manifest_dir.parent().unwrap_or(&manifest_dir);
    let app_env = env::var("APP_ENV").unwrap_or_else(|_| "prod".to_string());
    let env_file_name = format!(".env.{}", app_env.trim());
    let env_path = project_root.join("env").join(&env_file_name);
    let fallback_prod_path = project_root.join("env").join(".env.prod");

    // 让 cargo 在 env 文件变化时重跑 build.rs
    println!("cargo:rerun-if-env-changed=APP_ENV");
    println!("cargo:rerun-if-changed={}", env_path.display());
    println!("cargo:rerun-if-changed={}", fallback_prod_path.display());

    let contents = fs::read_to_string(&env_path).or_else(|_| fs::read_to_string(&fallback_prod_path));
    let mut entries: Vec<(String, String)> = Vec::new();

    if let Ok(contents) = contents {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_string();
                let mut val = v.trim().to_string();
                // 去掉两侧引号
                if (val.starts_with('"') && val.ends_with('"'))
                    || (val.starts_with('\'') && val.ends_with('\''))
                {
                    val = val[1..val.len() - 1].to_string();
                }
                if !key.is_empty() && !val.is_empty() {
                    entries.push((key, val));
                }
            }
        }
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo");
    let out_path = PathBuf::from(out_dir).join("built_env.rs");
    let mut code = format!("/// Auto-generated from {} at build time\n", env_file_name);
    code.push_str("#[allow(dead_code)]\npub fn get(key: &str) -> Option<&'static str> {\n");
    code.push_str("    match key {\n");
    for (k, v) in entries {
        code.push_str(&format!(
            "        \"{}\" => Some(\"{}\"),\n",
            k,
            v.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    let _ = fs::write(out_path, code);
}
