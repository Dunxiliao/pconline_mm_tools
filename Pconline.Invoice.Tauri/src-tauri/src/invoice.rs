use serde::{Deserialize, Serialize};
use crate::amazon;
use crate::fedex;
use crate::ontrac;
use crate::temu;
use crate::ups;
use calamine::{open_workbook, Data, Range, Reader, Xlsx};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoDto {
    pub file_full_name: String,
    pub file_name: String,
    pub status: String,
    pub row_count: i32,
    pub carrier: String,
    pub account_number: Option<String>,
    pub invoice_number: Option<String>,
}

/// 各承运商策略通用接口
pub trait CarrierStrategy {
    type ParsedData;

    fn get_file_info(&self, path: &str) -> Result<(FileInfoDto, Self::ParsedData), String>;
}

pub(crate) fn is_csv_file(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
}

/// Validate required headers and report the exact columns missing from the file.
pub(crate) fn validate_required_headers(
    carrier: &str,
    actual_headers: &[String],
    required_headers: &[&str],
    case_sensitive: bool,
) -> Result<(), String> {
    let mut missing = Vec::new();
    for required in required_headers {
        let present = actual_headers.iter().any(|actual| {
            let actual = actual.trim();
            if case_sensitive {
                actual == *required
            } else {
                actual.eq_ignore_ascii_case(required)
            }
        });
        if !present {
            missing.push(*required);
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{carrier} invoice file is missing required header column(s): {}. Confirm that the selected carrier is correct and compare the first-row column names with the {carrier} invoice template.",
            missing.join(", ")
        ))
    }
}

/// 将 CSV 和 Excel 的首个工作表统一为 calamine Range，供各承运商复用同一套解析逻辑。
pub(crate) fn read_first_sheet(path: &str) -> Result<Range<Data>, String> {
    if is_csv_file(path) {
        return read_csv_range(path);
    }

    let mut workbook: Xlsx<std::io::BufReader<std::fs::File>> =
        open_workbook(path).map_err(|e: calamine::XlsxError| e.to_string())?;
    let sheet_name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| "No worksheet found".to_string())?;
    workbook
        .worksheet_range(&sheet_name)
        .map_err(|e: calamine::XlsxError| e.to_string())
}

fn read_csv_range(path: &str) -> Result<Range<Data>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("Failed to open CSV file: {e}"))?;

    let mut rows = Vec::new();
    let mut max_columns = 0usize;
    for (row_index, record) in reader.records().enumerate() {
        let record = record.map_err(|e| format!("Failed to read CSV row {}: {e}", row_index + 1))?;
        max_columns = max_columns.max(record.len());
        rows.push(record);
    }

    if rows.is_empty() || max_columns == 0 {
        return Ok(Range::empty());
    }

    let mut range = Range::new(
        (0, 0),
        ((rows.len() - 1) as u32, (max_columns - 1) as u32),
    );
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, value) in row.iter().enumerate() {
            let value = if row_index == 0 && column_index == 0 {
                value.trim_start_matches('\u{feff}')
            } else {
                value
            };
            if !value.is_empty() {
                range.set_value(
                    (row_index as u32, column_index as u32),
                    Data::String(value.to_string()),
                );
            }
        }
    }
    Ok(range)
}

pub enum Carrier {
    Amazon(amazon::AmazonStrategy),
    Temu(temu::TemuStrategy),
    Ups(ups::UpsStrategy),
    Fedex(fedex::FedexStrategy),
    OnTrac(ontrac::OnTracStrategy),
}

impl Carrier {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Amazon" => Some(Self::Amazon(amazon::AmazonStrategy::new())),
            "Temu" => Some(Self::Temu(temu::TemuStrategy::new())),
            "UPS" => Some(Self::Ups(ups::UpsStrategy::new())),
            "Fedex" => Some(Self::Fedex(fedex::FedexStrategy::new())),
            "OnTrac" => Some(Self::OnTrac(ontrac::OnTracStrategy::new())),
            _ => None,
        }
    }
}

/// 解析 Excel/CSV 文件并生成发票文件信息列表
#[tauri::command]
pub async fn invoice_process_files(
    carrier: String,
    file_paths: Vec<String>,
) -> Result<Vec<FileInfoDto>, String> {
    let carrier = Carrier::from_name(&carrier).ok_or("Unsupported carrier")?;

    let mut result = Vec::new();
    for path in file_paths {
        let file_info = (match &carrier {
            Carrier::Amazon(s) => s.get_file_info(&path).map(|(file_info, _)| file_info),
            Carrier::Temu(s) => s.get_file_info(&path).map(|(file_info, _)| file_info),
            Carrier::Ups(s) => s.get_file_info(&path).map(|(file_info, _)| file_info),
            Carrier::Fedex(s) => s.get_file_info(&path).map(|(file_info, _)| file_info),
            Carrier::OnTrac(s) => s.get_file_info(&path).map(|(file_info, _)| file_info),
        })
        .map_err(|error| {
            let file_name = Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&path);
            format!("{file_name}: {error}")
        })?;
        result.push(file_info);
    }
    Ok(result)
}

/// 校验发票是否已存在（按承运商查 DB，更新 status 为 Exists/Pass）
#[tauri::command]
pub async fn invoice_check_files(
    _carrier: String,
    files: Vec<FileInfoDto>,
) -> Result<Vec<FileInfoDto>, String> {
    let mut out = Vec::with_capacity(files.len());
    for mut f in files {
        let path = &f.file_full_name;
        match f.carrier.as_str() {
            "Amazon" => {
                let (_, parsed) = match amazon::AmazonStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                let invoice_number = parsed.metadata.invoice_number.clone();
                if invoice_number.is_empty() {
                    f.status = "Pass".to_string();
                } else {
                    match crate::db::check_amazon_invoice_exists(&invoice_number).await {
                        Ok(existing) => {
                            f.status = if existing.is_empty() {
                                "Pass".to_string()
                            } else {
                                format!("Exists ({})", existing.join(", "))
                            };
                        }
                        Err(e) => f.status = format!("Check error: {}", e),
                    }
                }
            }
            "Temu" => f.status = "Pass".to_string(),
            "UPS" => {
                let (_, parsed) = match ups::UpsStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                let nums: std::collections::HashSet<String> = parsed.iter().map(|i| i.invoicenum.clone()).collect();
                let invoice_number = nums.into_iter().collect::<Vec<_>>().join("\n");
                if invoice_number.is_empty() {
                    f.status = "Pass".to_string();
                } else {
                    match crate::db::check_ups_invoice_exists(&invoice_number).await {
                        Ok(existing) => {
                            f.status = if existing.is_empty() {
                                "Pass".to_string()
                            } else {
                                format!("Exists ({})", existing.join(", "))
                            };
                        }
                        Err(e) => f.status = format!("Check error: {}", e),
                    }
                }
            }
            "Fedex" => {
                let (_, parsed) = match fedex::FedexStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                let nums: std::collections::HashSet<String> = parsed.invoices.iter().map(|i| i.invoice_number.clone()).collect();
                let invoice_number = nums.into_iter().collect::<Vec<_>>().join("\n");
                if invoice_number.is_empty() {
                    f.status = "Pass".to_string();
                } else {
                    match crate::db::check_fedex_invoice_exists(&invoice_number).await {
                        Ok(existing) => {
                            f.status = if existing.is_empty() {
                                "Pass".to_string()
                            } else {
                                format!("Exists ({})", existing.join(", "))
                            };
                        }
                        Err(e) => f.status = format!("Check error: {}", e),
                    }
                }
            }
            "OnTrac" => {
                let (_, parsed) = match ontrac::OnTracStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                let nums: std::collections::HashSet<String> =
                    parsed.iter().map(|i| i.invoice_number.clone()).collect();
                let invoice_number = nums.into_iter().collect::<Vec<_>>().join("\n");
                if invoice_number.is_empty() {
                    f.status = "Pass".to_string();
                } else {
                    match crate::db::check_ontrac_invoice_exists(&invoice_number).await {
                        Ok(existing) => {
                            f.status = if existing.is_empty() {
                                "Pass".to_string()
                            } else {
                                format!("Exists ({})", existing.join(", "))
                            };
                        }
                        Err(e) => f.status = format!("Check error: {}", e),
                    }
                }
            }
            _ => f.status = format!("Unsupported carrier: {}", f.carrier),
        }
        out.push(f);
    }
    Ok(out)
}

/// 上传发票到数据库（按承运商插入，更新 status 为 Successful/Failed）
#[tauri::command]
pub async fn invoice_upload_files(
    _carrier: String,
    files: Vec<FileInfoDto>,
) -> Result<Vec<FileInfoDto>, String> {
    let mut out = Vec::with_capacity(files.len());
    for mut f in files {
        let path = &f.file_full_name;
        match f.carrier.as_str() {
            "Amazon" => {
                let (_, parsed) = match amazon::AmazonStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                if parsed.items.is_empty() {
                    f.status = "Failed: no items".to_string();
                } else {
                    match crate::db::insert_amazon_invoice(
                        &parsed.metadata,
                        &parsed.items,
                        &parsed.adjustments,
                    )
                    .await
                    {
                        Ok(()) => f.status = "Successful".to_string(),
                        Err(e) => f.status = format!("Failed: {}", e),
                    }
                }
            }
            "Temu" => {
                let (_, parsed) = match temu::TemuStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                if parsed.is_empty() {
                    f.status = "Failed: no items".to_string();
                } else {
                    match crate::db::insert_temu_invoice(&parsed).await {
                        Ok(()) => f.status = "Successful".to_string(),
                        Err(e) => f.status = format!("Failed: {}", e),
                    }
                }
            }
            "UPS" => {
                let (_, parsed) = match ups::UpsStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                if parsed.is_empty() {
                    f.status = "Failed: no items".to_string();
                } else {
                    match crate::db::insert_ups_invoice(&parsed).await {
                        Ok(()) => f.status = "Successful".to_string(),
                        Err(e) => f.status = format!("Failed: {}", e),
                    }
                }
            }
            "Fedex" => {
                let (_, parsed) = match fedex::FedexStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                if parsed.invoices.is_empty() {
                    f.status = "Failed: no items".to_string();
                } else {
                    match crate::db::insert_fedex_invoice(&parsed).await {
                        Ok(()) => f.status = "Successful".to_string(),
                        Err(e) => f.status = format!("Failed: {}", e),
                    }
                }
            }
            "OnTrac" => {
                let (_, parsed) = match ontrac::OnTracStrategy::new().get_file_info(path) {
                    Ok(x) => x,
                    Err(e) => {
                        f.status = format!("Error: {}", e);
                        out.push(f);
                        continue;
                    }
                };
                if parsed.is_empty() {
                    f.status = "Failed: no items".to_string();
                } else {
                    match crate::db::insert_ontrac_invoice(&parsed).await {
                        Ok(()) => f.status = "Successful".to_string(),
                        Err(e) => f.status = format!("Failed: {}", e),
                    }
                }
            }
            _ => f.status = format!("Unsupported carrier: {}", f.carrier),
        }
        out.push(f);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static FILE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    fn with_csv<T>(content: &str, test: impl FnOnce(&str) -> T) -> T {
        let sequence = FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pconline-invoice-{}-{sequence}.csv",
            std::process::id()
        ));
        std::fs::write(&path, content).expect("write test CSV");
        let result = test(path.to_str().expect("UTF-8 temp path"));
        let _ = std::fs::remove_file(path);
        result
    }

    #[test]
    fn parses_temu_csv_with_bom_and_quoted_comma() {
        with_csv(
            "\u{feff}transaction-type,related-id,order id,order item id,sku,total\nSale,R1,O1,I1,\"SKU,1\",12.34\n",
            |path| {
                let (file, items) = temu::TemuStrategy::new().get_file_info(path).unwrap();
                assert_eq!(file.row_count, 1);
                assert_eq!(items[0].sku, "SKU,1");
            },
        );
    }

    #[test]
    fn parses_ups_csv() {
        with_csv(
            "Track_Num,Track_Num_Alt,Act_Num,Act_Num_Shipped,InvoiceNum,UserID\n1Z1,ALT,A1,A1,INV1,user\n",
            |path| {
                let (file, items) = ups::UpsStrategy::new().get_file_info(path).unwrap();
                assert_eq!(file.invoice_number.as_deref(), Some("INV1"));
                assert_eq!(items[0].track_num, "1Z1");
            },
        );
    }

    #[test]
    fn parses_fedex_csv() {
        with_csv(
            "Consolidated Account,Bill to Account Number,Invoice Date,Invoice Number,Original Amount Due,Express or Ground Tracking ID\nC1,B1,2026-09-01,INV1,10.00,TRACK1\n",
            |path| {
                let (file, info) = fedex::FedexStrategy::new().get_file_info(path).unwrap();
                assert_eq!(file.account_number.as_deref(), Some("B1"));
                assert_eq!(info.invoices[0].express_or_ground_tracking_id, "TRACK1");
            },
        );
    }

    #[test]
    fn parses_amazon_csv_as_single_sheet() {
        with_csv(
            "metadata\n,Amazon Account,,,,,INV1\n,ACC1,,,,,2026-09-01\n,2026-09-01 to 2026-09-07,,,,,2026-09-15\nTracking Id,Order ID\nTRACK1,ORDER1\n",
            |path| {
                let (file, info) = amazon::AmazonStrategy::new().get_file_info(path).unwrap();
                assert_eq!(file.invoice_number.as_deref(), Some("INV1"));
                assert_eq!(info.items[0].tracking_id, "TRACK1");
                assert!(info.adjustments.is_empty());
            },
        );
    }
}
