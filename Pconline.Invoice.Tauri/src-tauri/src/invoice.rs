use serde::{Deserialize, Serialize};
use crate::amazon;
use crate::fedex;
use crate::ontrac;
use crate::temu;
use crate::ups;

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

/// 解析 Excel 文件并生成发票文件信息列表
#[tauri::command]
pub async fn invoice_process_files(
    carrier: String,
    file_paths: Vec<String>,
) -> Result<Vec<FileInfoDto>, String> {
    let carrier = Carrier::from_name(&carrier).ok_or("Unsupported carrier")?;

    let mut result = Vec::new();
    for path in file_paths {
        let file_info = match &carrier {
            Carrier::Amazon(s) => s.get_file_info(&path)?.0,
            Carrier::Temu(s) => s.get_file_info(&path)?.0,
            Carrier::Ups(s) => s.get_file_info(&path)?.0,
            Carrier::Fedex(s) => s.get_file_info(&path)?.0,
            Carrier::OnTrac(s) => s.get_file_info(&path)?.0,
        };
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
