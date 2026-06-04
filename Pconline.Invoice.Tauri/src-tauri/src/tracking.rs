// Tracking Number 回传：解析 Excel + 调用 Etailflow V2 POST /api/orders/shipping

use calamine::{open_workbook, Data, Reader, Xlsx};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tauri::Manager;

use crate::etailflow::{self, OrderLine, Package, ShipOrderRequest};

fn normalize(s: &str) -> String {
    let s = s.replace("\r\n", " ").replace('\n', " ").replace('\r', " ");
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingOrderDto {
    pub order_number: String,
    pub tracking_number: String,
    pub carrier_name: String,
    pub service_code: String,
    pub status: String,
    pub packages: Vec<TrackingPackageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingPackageDto {
    pub sku: String,
    pub qty: i32,
    pub tracking_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub provider: i32,
    pub provider_name: String,
    pub seller: String,
}

/// 从环境变量或内嵌配置读取 ETAILFLOW_MERCHANT_CODE，未设置时默认 pconline（页面不展示，仅上传时使用）
#[tauri::command]
pub fn get_etailflow_merchant_code() -> String {
    crate::get_config("ETAILFLOW_MERCHANT_CODE").unwrap_or_else(|| "pconline".to_string())
}

/// 从 .env 或环境变量读取 APP_VERSION，与 lib.rs load_dotenv 路径一致
const DEFAULT_PLATFORMS_JSON: &str = include_str!("../resources/platforms.json");

fn load_platform_list() -> Vec<PlatformInfo> {
    // 1. Try exe directory (packaged app: user can place platforms.json next to .exe)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let path = dir.join("platforms.json");
            if path.exists() {
                if let Ok(s) = std::fs::read_to_string(&path) {
                    if let Ok(list) = serde_json::from_str::<Vec<PlatformInfo>>(&s) {
                        return list;
                    }
                }
            }
        }
    }
    // 2. Fallback: embedded default
    serde_json::from_str(DEFAULT_PLATFORMS_JSON).unwrap_or_else(|_| vec![
        PlatformInfo {
            provider: 4,
            provider_name: "Walmart".to_string(),
            seller: "Walmart".to_string(),
        },
        PlatformInfo {
            provider: 8,
            provider_name: "Newegg".to_string(),
            seller: "B0GJ".to_string(),
        },
        PlatformInfo {
            provider: 8,
            provider_name: "Newegg Business".to_string(),
            seller: "V70A".to_string(),
        },
        PlatformInfo {
            provider: 64,
            provider_name: "Goflow".to_string(),
            seller: "".to_string(),
        },
        PlatformInfo {
            provider: 2,
            provider_name: "Shipstation".to_string(),
            seller: "".to_string(),
        },
        PlatformInfo {
            provider: 32,
            provider_name: "Shein mmm".to_string(),
            seller: "".to_string(),
        },
    ])
}

#[tauri::command]
pub fn platform_list() -> Vec<PlatformInfo> {
    load_platform_list()
}

/// Pull Database：按平台从 Odoo 拉待回传订单号，再从 EfShip 拉出运数据
#[tauri::command]
pub async fn tracking_pull_database(provider: i32) -> Result<Vec<TrackingOrderDto>, String> {
    let order_numbers = crate::tracking_db::odoo_get_order_numbers(provider).await?;
    if order_numbers.is_empty() {
        return Ok(Vec::new());
    }
    let mut orders = crate::tracking_db::efship_get_orders(&order_numbers, provider).await?;
    // Gift orders (仅 Goflow provider=64)
    let gift_pairs = crate::tracking_db::odoo_get_gift_orders(&order_numbers, provider).await?;
    for (name, original_order_name) in gift_pairs {
        if let Some(order) = orders.iter().find(|o| o.order_number == name) {
            let mut new_order = order.clone();
            new_order.order_number = original_order_name;
            orders.push(new_order);
        }
    }
    Ok(orders)
}

/// Assign Order：按 Shipping Label Log 名称或订单号文本框拉取订单，再从 EfShip 拉出运数据
#[tauri::command]
pub async fn tracking_assign_order(
    provider: i32,
    shipping_label_log_name: Option<String>,
    order_number_lines: Option<String>,
) -> Result<Vec<TrackingOrderDto>, String> {
    let label = shipping_label_log_name.as_deref().unwrap_or("").trim();
    let lines: Vec<String> = order_number_lines
        .as_deref()
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if !label.is_empty() && !lines.is_empty() {
        return Err("Order number and Shipping Label Log cannot be specified at the same time.".to_string());
    }
    let order_numbers: Vec<String> = if !label.is_empty() {
        crate::tracking_db::odoo_get_order_numbers_by_label_name(label).await?
    } else if !lines.is_empty() {
        lines
    } else {
        return Err("Please enter Shipping Label Log Name or Order Number.".to_string());
    };
    if order_numbers.is_empty() {
        return Err("Not Match Order Number".to_string());
    }
    let gift_pairs = crate::tracking_db::odoo_get_gift_orders(&order_numbers, provider).await?;
    let mut orders = crate::tracking_db::efship_get_orders(&order_numbers, provider).await?;
    for (name, original_order_name) in gift_pairs {
        if let Some(order) = orders.iter().find(|o| o.order_number == name) {
            let mut new_order = order.clone();
            new_order.order_number = original_order_name;
            orders.push(new_order);
        }
    }
    Ok(orders)
}

fn get_cell(row: &[Data], map: &HashMap<String, usize>, keys: &[&str]) -> String {
    for key in keys {
        let k = normalize(key);
        if let Some(&col) = map.get(&k) {
            if let Some(c) = row.get(col) {
                let s = c.to_string().trim().to_string();
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    String::new()
}

fn get_cell_i32(row: &[Data], map: &HashMap<String, usize>, keys: &[&str]) -> i32 {
    let s = get_cell(row, map, keys);
    s.parse::<i32>().unwrap_or(0)
}

/// 解析 Tracking 回传用 Excel：首行为表头，支持 "Purchase order#" 或 "OrderNumber" 等列名，按订单号分组。
#[tauri::command]
pub fn tracking_parse_excel(file_paths: Vec<String>) -> Result<Vec<TrackingOrderDto>, String> {
    let mut all_orders: HashMap<String, TrackingOrderDto> = HashMap::new();
    for path in &file_paths {
        let path = Path::new(path);
        let mut workbook: Xlsx<std::io::BufReader<std::fs::File>> =
            open_workbook(path).map_err(|e: calamine::XlsxError| e.to_string())?;
        let sheet_name = workbook
            .sheet_names()
            .get(0)
            .cloned()
            .ok_or_else(|| "No worksheet found".to_string())?;
        let range: calamine::Range<Data> = workbook
            .worksheet_range(&sheet_name)
            .map_err(|e: calamine::XlsxError| e.to_string())?;
        let rows: Vec<_> = range.rows().map(|r| r.to_vec()).collect();
        if rows.is_empty() {
            continue;
        }
        let header_row = &rows[0];
        let mut map: HashMap<String, usize> = HashMap::new();
        for (idx, c) in header_row.iter().enumerate() {
            let name = normalize(&c.to_string());
            if !name.is_empty() {
                map.insert(name, idx);
            }
        }
        for row in rows.iter().skip(1) {
            let order_number =
                get_cell(row, &map, &["Purchase order#", "OrderNumber", "Order number"]);
            if order_number.is_empty() {
                continue;
            }
            let qty = get_cell_i32(row, &map, &["Qty", "Quantity"]);
            let sku = get_cell(row, &map, &["Sku", "SKU"]);
            let tracking_number =
                get_cell(row, &map, &["TrackingNumber", "Tracking number", "Tracking#"]);
            let carrier_name = get_cell(row, &map, &["CarrierName", "Carrier name", "Carrier"]);
            let service_code = get_cell(row, &map, &["ServiceCode", "Service code", "Service"]);

            let entry = all_orders.entry(order_number.clone()).or_insert_with(|| {
                TrackingOrderDto {
                    order_number: order_number.clone(),
                    tracking_number: String::new(),
                    carrier_name: carrier_name.clone(),
                    service_code: service_code.clone(),
                    status: "".to_string(),
                    packages: Vec::new(),
                }
            });
            if !tracking_number.is_empty() {
                entry.packages.push(TrackingPackageDto {
                    sku,
                    qty,
                    tracking_number: tracking_number.clone(),
                });
            }
        }
    }
    for o in all_orders.values_mut() {
        let set: std::collections::HashSet<String> = o
            .packages
            .iter()
            .map(|p| p.tracking_number.clone())
            .collect();
        o.tracking_number = set.into_iter().collect::<Vec<_>>().join(",");
    }
    let list: Vec<TrackingOrderDto> = all_orders.into_values().collect();
    Ok(list)
}

/// 从 resources 拷贝 Tracking Callback 模板到用户选择的路径
#[tauri::command]
pub fn tracking_download_template(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri::path::BaseDirectory;
    let resource_path = app
        .path()
        .resolve("resources/Tracking Callback.xlsx", BaseDirectory::Resource)
        .ok()
        .filter(|p: &std::path::PathBuf| p.exists())
        .or_else(|| {
            std::env::var("CARGO_MANIFEST_DIR").ok().and_then(|dir| {
                let p = std::path::Path::new(&dir).join("resources").join("Tracking Callback.xlsx");
                if p.exists() {
                    Some(p)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| "Template file not found.".to_string())?;
    std::fs::copy(&resource_path, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn convert_to_ship_requests(orders: &[TrackingOrderDto], replace_tracking: bool) -> Vec<ShipOrderRequest> {
    orders
        .iter()
        .map(|o| {
            let packages: Vec<Package> = o
                .packages
                .iter()
                .fold(
                    HashMap::<String, Vec<OrderLine>>::new(),
                    |mut acc, p| {
                        acc.entry(p.tracking_number.clone())
                            .or_default()
                            .push(OrderLine {
                                sku: p.sku.clone(),
                                quantity: p.qty,
                            });
                        acc
                    },
                )
                .into_iter()
                .map(|(tracking_number, lines)| Package {
                    tracking_number,
                    lines,
                })
                .collect();
            ShipOrderRequest {
                order_number: o.order_number.clone(),
                carrier_name: o.carrier_name.clone(),
                service_code: o.service_code.clone(),
                packages,
                replace_tracking_number: replace_tracking,
            }
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingPostItemResult {
    pub order_number: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackingPostResult {
    pub success: bool,
    pub message: Option<String>,
    pub success_order_numbers: Vec<String>,
    pub failed_order_numbers: Vec<String>,
    pub data: Vec<TrackingPostItemResult>,
}

/// 仅使用 Etailflow V2 方式回传：POST /api/orders/shipping
#[tauri::command]
pub async fn tracking_post_etailflow(
    access_token: String,
    merchant_code: String,
    service_provider: i32,
    seller: String,
    orders: Vec<TrackingOrderDto>,
    replace_tracking: bool,
) -> Result<TrackingPostResult, String> {
    if access_token.trim().is_empty() {
        return Err("Access token is required for Etailflow requests.".to_string());
    }
    if orders.is_empty() {
        return Ok(TrackingPostResult {
            success: false,
            message: Some("No orders to upload.".to_string()),
            success_order_numbers: Vec::new(),
            failed_order_numbers: Vec::new(),
            data: Vec::new(),
        });
    }
    let shippings = convert_to_ship_requests(&orders, replace_tracking);
    let result = etailflow::post_shipping_order(
        &access_token,
        &merchant_code,
        service_provider,
        &seller,
        shippings,
    )
    .await?;

    if !result.success {
        return Ok(TrackingPostResult {
            success: false,
            message: result.message,
            success_order_numbers: Vec::new(),
            failed_order_numbers: orders.iter().map(|o| o.order_number.clone()).collect(),
            data: Vec::new(),
        });
    }

    let data = result.data.unwrap_or_default();
    let success_order_numbers: Vec<String> = data
        .iter()
        .filter(|r| r.status.eq_ignore_ascii_case("success"))
        .map(|r| r.order_number.clone())
        .collect();
    let failed_order_numbers: Vec<String> = data
        .iter()
        .filter(|r| !r.status.eq_ignore_ascii_case("success"))
        .map(|r| r.order_number.clone())
        .collect();

    Ok(TrackingPostResult {
        success: true,
        message: result.message,
        success_order_numbers,
        failed_order_numbers,
        data: data
            .into_iter()
            .map(|r| TrackingPostItemResult {
                order_number: r.order_number,
                status: r.status,
                message: r.message,
            })
            .collect(),
    })
}

/// 写入前端生成的 xlsx bytes（base64）到指定路径
#[tauri::command]
pub fn tracking_write_xlsx(path: String, base64_xlsx: String) -> Result<(), String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let bytes = STANDARD.decode(base64_xlsx).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())
}
