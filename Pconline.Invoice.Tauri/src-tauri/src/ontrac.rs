//! OnTrac 承运商：表头第 0 行，ReadByHeader<OnTracInvoice>，Check 校验格式并查重 invoice_number

use crate::invoice::{CarrierStrategy, FileInfoDto};
use calamine::{open_workbook, Data, Reader, Xlsx};
use chrono::{Datelike, Timelike};
use std::collections::HashMap;
use std::path::Path;

pub struct OnTracStrategy;

impl OnTracStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
pub struct OnTracInvoice {
    pub invoice_number: String,
    pub billing_date: String,
    pub ontrac_destination_facility_code: String,
    pub reference1: String,
    pub reference2: String,
    pub customer_order_number: String,
    pub third_party_account_number: String,
    pub tracking_number: String,
    pub shipper_company_name: String,
    pub shipper_street: String,
    pub shipper_city: String,
    pub shipper_state: String,
    pub shipper_country: String,
    pub shipper_postalcode: String,
    pub destination_contact: String,
    pub destination_street: String,
    pub destination_city: String,
    pub destination_state: String,
    pub destination_country: String,
    pub destination_postalcode: String,
    pub service_code: String,
    pub zone: String,
    pub return_to_sender: String,
    pub residential: String,
    pub irregular_category: String,
    pub proof_of_delivery_name: String,
    pub proof_of_delivery_datetime: String,
    pub first_scan_datetime: String,
    pub customer_account: String,
    pub injection_postalcode: String,
    pub weight: String,
    pub length: String,
    pub width: String,
    pub height: String,
    pub billed_weight: String,
    pub billed_length: String,
    pub billed_width: String,
    pub billed_height: String,
    pub dim_factor: String,
    pub weight_source: String,
    pub total_charges: String,
    pub service_charges: String,
    pub address_correction_surcharge: String,
    pub delivery_intervention_required: String,
    pub extra_piece_surcharge: String,
    pub residential_surcharge: String,
    pub delivery_area_surcharge: String,
    pub extended_area_surcharge: String,
    pub additional_handling_surcharge: String,
    pub large_package_surcharge: String,
    pub over_maximum_limits_surcharge: String,
    pub signature_required: String,
    pub adult_signature_required: String,
    pub relabel_surcharge: String,
    pub weekend_surcharge: String,
    pub demand_surcharge: String,
    pub demand_additional_handling_surcharge: String,
    pub demand_large_package_surcharge: String,
    pub demand_over_maximum_limits_surcharge: String,
    pub on_call_pickup: String,
    pub shipping_charge_correction_audit_fee: String,
    pub volume_rebate: String,
    pub volume_rebate_2: String,
    pub volume_rebate_3: String,
    pub missing_pld: String,
    pub other_adjustments: String,
    pub miscellaneous_charges: String,
    pub fuel_surcharge: String,
    pub upload_invoice_file_name: String,
    pub upload_invoice_file_date: String,
}

type HeaderMap = HashMap<String, usize>;

impl CarrierStrategy for OnTracStrategy {
    type ParsedData = Vec<OnTracInvoice>;

    fn get_file_info(&self, path: &str) -> Result<(FileInfoDto, Self::ParsedData), String> {
        let file_name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

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

        let header_row_index = 0usize;
        let header_row = range
            .rows()
            .nth(header_row_index)
            .ok_or_else(|| "Header row is missing".to_string())?;

        if !validate_header(header_row) {
            return Err(
                "File format does not match the selected carrier. Please double check your file and upload again."
                    .to_string(),
            );
        }

        let header_map = build_header_map(header_row);
        let mut items = parse_items(&range, header_row_index, &header_map)?;
        if items.is_empty() {
            return Err("No records found in Excel.".to_string());
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for inv in &mut items {
            inv.upload_invoice_file_name = file_name.clone();
            inv.upload_invoice_file_date = now.clone();
        }

        let mut seen_inv: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ordered_inv: Vec<String> = Vec::new();
        for i in &items {
            if seen_inv.insert(i.invoice_number.clone()) {
                ordered_inv.push(i.invoice_number.clone());
            }
        }

        let mut seen_acct: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ordered_acct: Vec<String> = Vec::new();
        for i in &items {
            if seen_acct.insert(i.customer_account.clone()) {
                ordered_acct.push(i.customer_account.clone());
            }
        }

        let file_info = FileInfoDto {
            file_full_name: path.to_string(),
            file_name,
            status: "Init".to_string(),
            row_count: items.len() as i32,
            carrier: "OnTrac".to_string(),
            account_number: Some(ordered_acct.join(",")).filter(|s| !s.is_empty()),
            invoice_number: Some(ordered_inv.join("\n")).filter(|s| !s.is_empty()),
        };

        Ok((file_info, items))
    }
}

fn normalize_column_name(input: &str) -> String {
    let s = input.replace("\r\n", " ").replace('\n', " ").replace('\r', " ");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_header_map(header_row: &[Data]) -> HeaderMap {
    header_row
        .iter()
        .enumerate()
        .filter_map(|(idx, c)| {
            let name = normalize_column_name(&c.to_string());
            if name.is_empty() {
                None
            } else {
                Some((name.to_lowercase(), idx))
            }
        })
        .collect()
}

fn get_cell(row: &[Data], map: &HeaderMap, name: &str) -> String {
    let key = normalize_column_name(name).to_lowercase();
    map.get(&key)
        .and_then(|&col| row.get(col))
        .map(|c| c.to_string().trim().to_string())
        .unwrap_or_default()
}

fn get_cell_date(row: &[Data], map: &HeaderMap, name: &str) -> String {
    let key = normalize_column_name(name).to_lowercase();
    let Some(cell) = map.get(&key).and_then(|&col| row.get(col)) else {
        return String::new();
    };
    match cell {
        Data::String(s) => s.trim().to_string(),
        Data::Float(serial) if is_excel_date_serial(*serial) => excel_serial_display(*serial),
        Data::Int(serial) if is_excel_date_serial(*serial as f64) => {
            excel_serial_display(*serial as f64)
        }
        other => excel_serial_from_text(&other.to_string()),
    }
}

fn is_excel_date_serial(n: f64) -> bool {
    (1.0..=60000.0).contains(&n)
}

/// 已是日期文本则原样返回；纯数字字符串则按 Excel 序列转换
fn excel_serial_from_text(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    if s.contains('-') || s.contains('/') || s.contains(':') {
        return s.to_string();
    }
    if let Ok(serial) = s.parse::<f64>() {
        if is_excel_date_serial(serial) {
            return excel_serial_display(serial);
        }
    }
    s.to_string()
}

/// Excel 日期格存的是序列号；转成与 Excel 显示一致的文本（如 2026/5/1，不补零、纯日期不带 00:00:00）
fn excel_serial_display(serial: f64) -> String {
    let Some(base) = chrono::NaiveDate::from_ymd_opt(1899, 12, 30) else {
        return serial.to_string();
    };
    let days = serial.floor() as i64;
    let frac = serial - serial.floor();
    let date = base + chrono::Duration::days(days);
    let mut out = format!("{}/{}/{}", date.year(), date.month(), date.day());
    if frac > 1e-6 {
        let secs = (frac * 86400.0).round() as i64;
        if let Some(time) = chrono::NaiveTime::from_hms_opt(0, 0, 0)
            .map(|t| t + chrono::Duration::seconds(secs))
        {
            out.push_str(&format!(
                " {}:{}:{}",
                time.hour(),
                time.minute(),
                time.second()
            ));
        }
    }
    out
}

fn validate_header(header_row: &[Data]) -> bool {
    let required = [
        "invoice number",
        "billing date",
        "ontrac destination facility code",
        "reference1",
        "reference2",
        "customer order number",
        "3rd party account number",
        "tracking number",
        "shipper company name",
        "shipper street",
        "shipper city",
        "shipper state",
        "shipper country",
        "shipper postalcode",
        "destination contact",
        "destination street",
        "destination city",
        "destination state",
        "destination country",
        "destination postalcode",
        "service code",
        "zone",
        "return to sender",
        "residential",
        "irregular category",
        "proof of delivery name",
        "proof of delivery datetime",
        "first scan date time",
        "customer account",
        "injection postalcode",
        "weight(lbs)",
        "length(in)",
        "width(in)",
        "height(in)",
        "billed weight (lbs)",
        "billed length(in)",
        "billed width(in)",
        "billed height(in)",
        "dim factor",
        "weight source",
        "total charges",
        "service charge",
        "address correction surcharge",
        "delivery intervention required",
        "extra piece surcharge",
        "residential surcharge",
        "delivery area surcharge",
        "extended area surcharge",
        "additional handling surcharge",
        "large package surcharge",
        "over maximum limits surcharge",
        "signature required",
        "adult signature required",
        "relabel surcharge",
        "weekend surcharge",
        "demand surcharge",
        "demand additional handling surcharge",
        "demand large package surcharge",
        "demand over maximum limits surcharge",
        "on call pickup",
        "shipping charge correction audit fee",
        "volume rebate",
        "volume rebate 2",
        "volume rebate 3",
        "missing pld",
        "other adjustments",
        "miscellaneous charges",
        "fuel surcharge",
    ];
    let headers: std::collections::HashSet<String> = header_row
        .iter()
        .map(|c| normalize_column_name(&c.to_string()).to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    required.iter().all(|h| headers.contains(*h))
}

fn parse_items(
    range: &calamine::Range<Data>,
    header_row_index: usize,
    map: &HeaderMap,
) -> Result<Vec<OnTracInvoice>, String> {
    let mut items = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        let tracking_number = get_cell(row, map, "Tracking Number");
        let invoice_number = get_cell(row, map, "Invoice Number");
        if tracking_number.is_empty() && invoice_number.is_empty() {
            continue;
        }
        items.push(OnTracInvoice {
            invoice_number,
            billing_date: get_cell_date(row, map, "Billing Date"),
            ontrac_destination_facility_code: get_cell(
                row,
                map,
                "Ontrac Destination Facility Code",
            ),
            reference1: get_cell(row, map, "Reference1"),
            reference2: get_cell(row, map, "Reference2"),
            customer_order_number: get_cell(row, map, "Customer Order Number"),
            third_party_account_number: get_cell(row, map, "3RD Party Account Number"),
            tracking_number,
            shipper_company_name: get_cell(row, map, "Shipper Company Name"),
            shipper_street: get_cell(row, map, "Shipper Street"),
            shipper_city: get_cell(row, map, "Shipper City"),
            shipper_state: get_cell(row, map, "Shipper State"),
            shipper_country: get_cell(row, map, "Shipper Country"),
            shipper_postalcode: get_cell(row, map, "Shipper Postalcode"),
            destination_contact: get_cell(row, map, "Destination Contact"),
            destination_street: get_cell(row, map, "Destination Street"),
            destination_city: get_cell(row, map, "Destination City"),
            destination_state: get_cell(row, map, "Destination State"),
            destination_country: get_cell(row, map, "Destination Country"),
            destination_postalcode: get_cell(row, map, "Destination Postalcode"),
            service_code: get_cell(row, map, "Service Code"),
            zone: get_cell(row, map, "Zone"),
            return_to_sender: get_cell(row, map, "Return to Sender"),
            residential: get_cell(row, map, "Residential"),
            irregular_category: get_cell(row, map, "Irregular Category"),
            proof_of_delivery_name: get_cell(row, map, "Proof of Delivery Name"),
            proof_of_delivery_datetime: get_cell_date(row, map, "Proof of Delivery DateTime"),
            first_scan_datetime: get_cell_date(row, map, "First Scan Date Time"),
            customer_account: get_cell(row, map, "Customer Account"),
            injection_postalcode: get_cell(row, map, "Injection Postalcode"),
            weight: get_cell(row, map, "Weight(lbs)"),
            length: get_cell(row, map, "Length(in)"),
            width: get_cell(row, map, "Width(in)"),
            height: get_cell(row, map, "Height(in)"),
            billed_weight: get_cell(row, map, "Billed Weight (lbs)"),
            billed_length: get_cell(row, map, "Billed Length(in)"),
            billed_width: get_cell(row, map, "Billed Width(in)"),
            billed_height: get_cell(row, map, "Billed Height(in)"),
            dim_factor: get_cell(row, map, "DIM Factor"),
            weight_source: get_cell(row, map, "Weight Source"),
            total_charges: get_cell(row, map, "Total Charges"),
            service_charges: get_cell(row, map, "Service Charge"),
            address_correction_surcharge: get_cell(row, map, "Address Correction Surcharge"),
            delivery_intervention_required: get_cell(row, map, "Delivery Intervention Required"),
            extra_piece_surcharge: get_cell(row, map, "Extra Piece Surcharge"),
            residential_surcharge: get_cell(row, map, "Residential Surcharge"),
            delivery_area_surcharge: get_cell(row, map, "Delivery Area Surcharge"),
            extended_area_surcharge: get_cell(row, map, "Extended Area Surcharge"),
            additional_handling_surcharge: get_cell(row, map, "Additional Handling Surcharge"),
            large_package_surcharge: get_cell(row, map, "Large Package Surcharge"),
            over_maximum_limits_surcharge: get_cell(row, map, "Over Maximum Limits Surcharge"),
            signature_required: get_cell(row, map, "Signature Required"),
            adult_signature_required: get_cell(row, map, "Adult Signature Required"),
            relabel_surcharge: get_cell(row, map, "Relabel Surcharge"),
            weekend_surcharge: get_cell(row, map, "Weekend Surcharge"),
            demand_surcharge: get_cell(row, map, "Demand Surcharge"),
            demand_additional_handling_surcharge: get_cell(
                row,
                map,
                "Demand Additional Handling Surcharge",
            ),
            demand_large_package_surcharge: get_cell(row, map, "Demand Large Package Surcharge"),
            demand_over_maximum_limits_surcharge: get_cell(
                row,
                map,
                "Demand Over Maximum Limits Surcharge",
            ),
            on_call_pickup: get_cell(row, map, "On Call Pickup"),
            shipping_charge_correction_audit_fee: get_cell(
                row,
                map,
                "Shipping Charge Correction Audit Fee",
            ),
            volume_rebate: get_cell(row, map, "Volume Rebate"),
            volume_rebate_2: get_cell(row, map, "Volume Rebate 2"),
            volume_rebate_3: get_cell(row, map, "Volume Rebate 3"),
            missing_pld: get_cell(row, map, "Missing PLD"),
            other_adjustments: get_cell(row, map, "Other Adjustments"),
            miscellaneous_charges: get_cell(row, map, "Miscellaneous Charges"),
            fuel_surcharge: get_cell(row, map, "Fuel Surcharge"),
            upload_invoice_file_name: String::new(),
            upload_invoice_file_date: String::new(),
        });
    }
    Ok(items)
}
