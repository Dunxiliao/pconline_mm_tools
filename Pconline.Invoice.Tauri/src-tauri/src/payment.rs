//! Payment 上传：Excel（3PL Operation Fee + 3PL Shipping）→ 校验 → 更新 WMS sale_order / sale_order_package 已付标记

use calamine::{open_workbook, Data, Reader, Xlsx};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

pub const SHEET_OPERATION_FEE: &str = "3PL Operation Fee";
pub const SHEET_OPERATION_FEE_ALT: &str = "Operation Fee";
pub const SHEET_SHIPPING: &str = "3PL Shipping";
pub const SHEET_SHIPPING_ALT: &str = "Shipping Fee";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentFileInfo {
    pub file_name: String,
    pub file_full_name: String,
    pub status: String,
    pub total: i32,
    pub success_count: i32,
    pub failed_count: i32,
    pub shipping_total: i32,
    pub shipping_success_count: i32,
    pub shipping_failed_count: i32,
    pub operation: Option<String>,
    pub shipping: Option<String>,
    pub message: Option<String>,
    pub operation_details: Option<Vec<OperationRowDetail>>,
    pub shipping_details: Option<Vec<ShippingRowDetail>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRowDetail {
    pub index: i32,
    pub order_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingRowDetail {
    pub index: i32,
    pub order_number: String,
    pub tracking_number: String,
    pub status: String,
    pub message: String,
}

fn normalize_column_name(s: &str) -> String {
    let s = s
        .replace("\r\n", " ")
        .replace('\n', " ")
        .replace('\r', " ");
    let s = s.trim();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 从文件名解析客户名：第一个下划线之前部分
pub fn get_customer_name_from_file_name(file_name: &str) -> String {
    let name = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim();
    name.split('_')
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| name.to_string())
}

fn get_cell_string(cell: Option<&Data>) -> String {
    match cell {
        None => String::new(),
        Some(Data::String(s)) => s.clone(),
        Some(Data::Float(f)) => {
            if (*f - f.round()).abs() < 1e-9 {
                format!("{}", *f as i64)
            } else {
                format!("{:.16}", f)
            }
        }
        Some(Data::Int(i)) => i.to_string(),
        Some(other) => other.to_string(),
    }
}

fn open_workbook_xlsx(path: &str) -> Result<Xlsx<std::io::BufReader<std::fs::File>>, String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if ext.eq_ignore_ascii_case("xlsx") || ext.eq_ignore_ascii_case("xlsm") {
        open_workbook(path).map_err(|e: calamine::XlsxError| e.to_string())
    } else if ext.eq_ignore_ascii_case("xls") {
        Err("xls 格式暂不支持，请使用 xlsx".to_string())
    } else {
        Err("仅支持 .xlsx 文件".to_string())
    }
}

fn get_operation_sheet(
    sheet_names: &[String],
    workbook: &mut Xlsx<std::io::BufReader<std::fs::File>>,
) -> Option<calamine::Range<Data>> {
    let name = sheet_names
        .iter()
        .find(|s| s.eq_ignore_ascii_case(SHEET_OPERATION_FEE) || s.eq_ignore_ascii_case(SHEET_OPERATION_FEE_ALT))?;
    workbook.worksheet_range(name).ok()
}

fn get_shipping_sheet(
    sheet_names: &[String],
    workbook: &mut Xlsx<std::io::BufReader<std::fs::File>>,
) -> Option<calamine::Range<Data>> {
    let name = sheet_names
        .iter()
        .find(|s| s.eq_ignore_ascii_case(SHEET_SHIPPING) || s.eq_ignore_ascii_case(SHEET_SHIPPING_ALT))?;
    workbook.worksheet_range(name).ok()
}

fn sheet_has_column(range: &calamine::Range<Data>, required: &str) -> bool {
    let (_, cols) = range.get_size();
    let required_norm = normalize_column_name(required);
    for c in 0..cols {
        let cell = range.get((0, c));
        let col_name = normalize_column_name(&get_cell_string(cell));
        if col_name.eq_ignore_ascii_case(&required_norm) {
            return true;
        }
    }
    false
}

/// Operation Fee：B 列(索引 1) 为 Order ID
fn read_order_ids_from_operation_sheet(range: &calamine::Range<Data>) -> Vec<String> {
    let (rows, _) = range.get_size();
    let mut list = Vec::new();
    for r in 1..rows {
        let cell = range.get((r, 1));
        let val = get_cell_string(cell).trim().to_string();
        if !val.is_empty() {
            list.push(val);
        }
    }
    list
}

/// Shipping：C 列(2) Order Number，D 列(3) Tracking Number
fn read_shipping_rows_from_sheet(range: &calamine::Range<Data>) -> Vec<(String, String)> {
    let (rows, _) = range.get_size();
    let mut list = Vec::new();
    for r in 1..rows {
        let row = range.get((r, 2));
        let track = range.get((r, 3));
        let order_num = get_cell_string(row).trim().to_string();
        let tracking = get_cell_string(track).trim().to_string();
        if order_num.is_empty() && tracking.is_empty() {
            continue;
        }
        list.push((order_num, tracking));
    }
    list
}

/// 校验单个文件：表存在、列存在、客户名在 existing_customers 中
pub fn validate(
    file_path: &str,
    file_name: &str,
    existing_customers: &HashSet<String>,
) -> Result<(), String> {
    if file_path.is_empty() {
        return Err("文件路径为空".to_string());
    }
    let path = Path::new(file_path);
    if !path.exists() {
        return Err("文件不存在".to_string());
    }
    let mut workbook = open_workbook_xlsx(file_path)?;
    let sheet_names = workbook.sheet_names().to_vec();
    let op_range = get_operation_sheet(&sheet_names, &mut workbook)
        .ok_or_else(|| format!("缺少工作表: {} / {}", SHEET_OPERATION_FEE, SHEET_OPERATION_FEE_ALT))?;
    let ship_range = get_shipping_sheet(&sheet_names, &mut workbook)
        .ok_or_else(|| format!("缺少工作表: {} / {}", SHEET_SHIPPING, SHEET_SHIPPING_ALT))?;

    let mut missing = Vec::new();
    if !sheet_has_column(&op_range, "Order ID") {
        missing.push("'3PL Operation Fee' 列 'Order ID'");
    }
    if !sheet_has_column(&ship_range, "Order Number") {
        missing.push("'3PL Shipping' 列 'Order Number'");
    }
    if !sheet_has_column(&ship_range, "Tracking Number") {
        missing.push("'3PL Shipping' 列 'Tracking Number'");
    }
    if !missing.is_empty() {
        return Err(format!("缺少必填列: {}", missing.join("; ")));
    }

    let customer = get_customer_name_from_file_name(file_name);
    if customer.is_empty() {
        return Err("无法从文件名解析客户名（期望 Name_xxx.xlsx）".to_string());
    }
    let normalized = customer.trim().to_lowercase();
    if !existing_customers.contains(&normalized) {
        return Err(format!("客户 '{}' 在数据库中不存在", customer));
    }
    Ok(())
}

/// 处理单个文件：读 Excel → 更新 WMS → 返回统计与明细
pub async fn process(file_path: &str) -> Result<ProcessResult, String> {
    if file_path.is_empty() {
        return Err("文件路径为空".to_string());
    }
    let path = Path::new(file_path);
    if !path.exists() {
        return Err("文件不存在".to_string());
    }
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut workbook = open_workbook_xlsx(file_path)?;
    let sheet_names = workbook.sheet_names().to_vec();
    let order_ids = {
        let op_range = get_operation_sheet(&sheet_names, &mut workbook)
            .ok_or_else(|| format!("缺少工作表: {} / {}", SHEET_OPERATION_FEE, SHEET_OPERATION_FEE_ALT))?;
        read_order_ids_from_operation_sheet(&op_range)
    };
    let shipping_rows = {
        let ship_range = get_shipping_sheet(&sheet_names, &mut workbook)
            .ok_or_else(|| format!("缺少工作表: {} / {}", SHEET_SHIPPING, SHEET_SHIPPING_ALT))?;
        read_shipping_rows_from_sheet(&ship_range)
    };
    let customer_code = get_customer_name_from_file_name(file_name);
    if customer_code.trim().is_empty() {
        return Err("无法从文件名解析客户 Code（期望 Name_xxx.xlsx）".to_string());
    }

    let (op_total, op_updated) =
        crate::db::payment_update_operation_fee_paid(&order_ids, &customer_code).await?;
    let matched_order_ids =
        crate::db::payment_get_matched_operation_order_ids(&order_ids, &customer_code).await?;

    let op_details: Vec<OperationRowDetail> = order_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let matched = matched_order_ids.contains(id.as_str());
            OperationRowDetail {
                index: (i + 1) as i32,
                order_id: id.clone(),
                status: if matched { "Matched" } else { "Failed" }.to_string(),
                message: if matched {
                    String::new()
                } else {
                    "Order not found or not updated".to_string()
                },
            }
        })
        .collect();

    let (ship_total, ship_r1, ship_r2, ship_unmatched, matched_trackings, round2_orders, multi_package) =
        crate::db::payment_update_shipping_fee_paid(&shipping_rows, &customer_code).await?;

    let round2_set: HashSet<String> = round2_orders.iter().map(|s| s.to_lowercase()).collect();
    let multi_set: HashSet<String> = multi_package.iter().map(|s| s.to_lowercase()).collect();

    let ship_details: Vec<ShippingRowDetail> = shipping_rows
        .iter()
        .enumerate()
        .map(|(i, (order_num, tracking))| {
            let order_num = order_num.trim();
            let tracking = tracking.trim();
            let (status, message) = if !tracking.is_empty() && matched_trackings.contains(tracking) {
                ("Matched", "(Tracking)")
            } else if !order_num.is_empty() && round2_set.contains(&order_num.to_lowercase()) {
                ("Matched", "(Order fallback)")
            } else {
                let msg = if !order_num.is_empty() && multi_set.contains(&order_num.to_lowercase()) {
                    "Multiple packages in WMS"
                } else {
                    "Not found in WMS"
                };
                ("Failed", msg)
            };
            ShippingRowDetail {
                index: (i + 1) as i32,
                order_number: order_num.to_string(),
                tracking_number: tracking.to_string(),
                status: status.to_string(),
                message: message.to_string(),
            }
        })
        .collect();

    Ok(ProcessResult {
        op_total,
        op_updated: op_updated as usize,
        ship_total,
        ship_r1,
        ship_r2,
        ship_unmatched,
        op_details,
        ship_details,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub op_total: usize,
    pub op_updated: usize,
    pub ship_total: usize,
    pub ship_r1: usize,
    pub ship_r2: usize,
    pub ship_unmatched: usize,
    pub op_details: Vec<OperationRowDetail>,
    pub ship_details: Vec<ShippingRowDetail>,
}

/// 选择文件后仅解析路径，返回文件列表（status 为空）
#[tauri::command]
pub async fn payment_add_files(file_paths: Vec<String>) -> Result<Vec<PaymentFileInfo>, String> {
    let mut result = Vec::new();
    for path in file_paths {
        if path.trim().is_empty() {
            continue;
        }
        let file_name = Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        result.push(PaymentFileInfo {
            file_name,
            file_full_name: path,
            status: String::new(),
            total: 0,
            success_count: 0,
            failed_count: 0,
            shipping_total: 0,
            shipping_success_count: 0,
            shipping_failed_count: 0,
            operation: None,
            shipping: None,
            message: None,
            operation_details: None,
            shipping_details: None,
        });
    }
    Ok(result)
}

/// 批量获取文件名对应的客户名在 DB 中存在的集合（小写）
#[tauri::command]
pub async fn payment_get_existing_customers(file_names: Vec<String>) -> Result<Vec<String>, String> {
    let customer_names: Vec<String> = file_names
        .iter()
        .map(|n| get_customer_name_from_file_name(n))
        .filter(|s| !s.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    crate::db::payment_get_existing_customers(&customer_names).await
}

/// 校验文件格式与客户存在性
#[tauri::command]
pub async fn payment_validate_file(
    file_path: String,
    file_name: String,
    existing_customers: Vec<String>,
) -> Result<(), String> {
    let set: HashSet<String> = existing_customers
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();
    validate(&file_path, &file_name, &set)
}

/// 处理单个文件（上传到 WMS）
#[tauri::command]
pub async fn payment_process_file(file_path: String) -> Result<ProcessResult, String> {
    process(&file_path).await
}
