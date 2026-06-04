use crate::invoice::{CarrierStrategy, FileInfoDto};
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct AmazonStrategy;

impl AmazonStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
pub struct AmazonInvoiceMetadata {
    pub invoice_number: String,
    pub account_name: String,
    pub account_number: String,
    pub service_period_start: String,
    pub service_period_end: String,
    pub invoice_date: String,
    pub due_date: String,
    pub upload_invoice_file_name: String,
    pub upload_invoice_file_date: String,
}

#[derive(Debug, Clone)]
pub struct AmazonInvoiceItem {
    pub tracking_id: String,
    pub order_id: String,
    pub reference: String,
    pub from_place: String,
    pub from_zip: String,
    pub to_place: String,
    pub to_zip: String,
    pub date_label_printed: String,
    pub shipping_service: String,
    pub date_of_charge: String,
    pub leg_type: String,
    pub comments: String,
    pub child_account_id: String,
    pub zone: String,
    pub base_rate_usd: String,
    pub discount_usd: String,
    pub delivery_area_surcharge_usd: String,
    pub fuel_surcharge_usd: String,
    pub additional_handeling_fee_usd: String,
    pub peak_demand_surcharge_usd: String,
    pub non_standard_fee_usd: String,
    pub pickup_charge_usd: String,
    pub tax_exclusive_total_charge_usd: String,
    pub tax_amount_usd: String,
    pub tax_inclusive_total_charge_usd: String,
    pub billable_weight: String,
    pub actual_weight: String,
    pub dimensional_divisor: String,
    pub length_in: String,
    pub width_in: String,
    pub height_in: String,
}

#[derive(Debug, Clone)]
pub struct AmazonInvoiceAdjustment {
    pub tracking_id: String,
    pub order_id: String,
    pub reference: String,
    pub date_label_printed: String,
    pub shipping_service: String,
    pub date_of_charge: String,
    pub charge_type: String,
    pub comments: String,
    pub base_rate_usd: String,
    pub tax_amount_usd: String,
    pub base_rate_incl_tax_usd: String,
}

#[derive(Debug, Clone)]
struct AmazonInvoiceItemV2 {
    child_account_id: String,
    tracking_id: String,
    order_id: String,
    reference: String,
    from_place: String,
    from_zip: String,
    to_place: String,
    to_zip: String,
    date_label_printed: String,
    date_of_charge: String,
    shipping_service: String,
    leg_type: String,
    charge_type: String,
    charge_code: String,
    comments: String,
    tax_exclusive_charge_value_usd: String,
    tax_amount_usd: String,
    tax_inclusive_charge_value_usd: String,
    zone: String,
    billable_weight: String,
    actual_weight: String,
    dimensional_divisor: String,
    length_in: String,
    width_in: String,
    height_in: String,
}

#[derive(Debug, Clone)]
pub struct AmazonInfo {
    pub metadata: AmazonInvoiceMetadata,
    pub items: Vec<AmazonInvoiceItem>,
    pub adjustments: Vec<AmazonInvoiceAdjustment>,
}

impl CarrierStrategy for AmazonStrategy {
    type ParsedData = AmazonInfo;

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

        let header_row_index = 4usize;
        let header_row = range
            .rows()
            .nth(header_row_index)
            .ok_or_else(|| "Header row is missing".to_string())?;

        let excel_headers: Vec<String> = header_row
            .iter()
            .map(|c| normalize_column_name(&c.to_string()))
            .filter(|s| !s.is_empty())
            .collect();

        if excel_headers.is_empty() {
            return Err("No headers found in Excel file.".to_string());
        }

        let (is_valid, msg) = validate_header_detailed(&excel_headers);
        if !is_valid {
            return Err(
                msg.unwrap_or_else(|| "File format does not match the selected carrier. Please double check your file and upload again.".to_string())
            );
        }

        let mut metadata = parse_metadata(&range)?;
        metadata.upload_invoice_file_name = file_name.clone();
        metadata.upload_invoice_file_date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let header_map = build_header_map(header_row);
        // 与 C# 一致：新版本按 V2 解析再 ConvertV2ToV1（按 tracking_id 聚合成一条）；旧版本直接一行一条
        let is_v2 = is_new_version(&excel_headers);
        let (items, adjustments) = if is_v2 {
            let items_v2 = parse_items_v2(&range, header_row_index, &header_map)?;
            (
                convert_v2_to_v1(items_v2.clone())?,
                extract_adjustments_from_v2(&items_v2),
            )
        } else {
            (
                parse_items(&range, header_row_index, &header_map)?,
                parse_adjustments_v1(&mut workbook).unwrap_or_default(),
            )
        };
        let row_count = items.len() as i32;

        let file_info = FileInfoDto {
            file_full_name: path.to_string(),
            file_name,
            status: "Init".to_string(),
            row_count,
            carrier: "Amazon".to_string(),
            account_number: Some(metadata.account_number.clone()).filter(|s| !s.is_empty()),
            invoice_number: Some(metadata.invoice_number.clone()).filter(|s| !s.is_empty()),
        };

        Ok((
            file_info,
            AmazonInfo {
                metadata,
                items,
                adjustments,
            },
        ))
    }
}

type HeaderMap = HashMap<String, usize>;

fn get_cell_string(range: &calamine::Range<Data>, row: u32, col: u32) -> String {
    range
        .get((row as usize, col as usize))
        .map(|c| c.to_string())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn parse_metadata(
    range: &calamine::Range<Data>,
) -> Result<AmazonInvoiceMetadata, String> {
    let account_name = get_cell_string(range, 1, 1);
    let account_number = get_cell_string(range, 2, 1);
    let service_period = get_cell_string(range, 3, 1);
    let invoice_number = get_cell_string(range, 1, 6);
    let invoice_date = get_cell_string(range, 2, 6);
    let due_date = get_cell_string(range, 3, 6);

    let (service_period_start, service_period_end) = match service_period.split(" to ").map(str::trim).collect::<Vec<_>>().as_slice() {
        [a, b] => (a.to_string(), b.to_string()),
        _ => (String::new(), String::new()),
    };

    Ok(AmazonInvoiceMetadata {
        invoice_number,
        account_name,
        account_number,
        service_period_start,
        service_period_end,
        invoice_date,
        due_date,
        upload_invoice_file_name: String::new(),
        upload_invoice_file_date: String::new(),
    })
}

fn build_header_map(header_row: &[calamine::Data]) -> HeaderMap {
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

fn get_cell(row: &[calamine::Data], map: &HeaderMap, name: &str) -> String {
    let key = normalize_column_name(name).to_lowercase();
    map.get(&key)
        .and_then(|&col| row.get(col))
        .map(|c| c.to_string().trim().to_string())
        .unwrap_or_default()
}

fn get_cell_alt(row: &[calamine::Data], map: &HeaderMap, old: &str, new: &str) -> String {
    let v = get_cell(row, map, old);
    if v.is_empty() {
        get_cell(row, map, new)
    } else {
        v
    }
}

fn parse_items(range: &calamine::Range<Data>, header_row_index: usize, map: &HeaderMap) -> Result<Vec<AmazonInvoiceItem>, String> {
    let mut items = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        let tracking_id = get_cell(row, map, "Tracking Id");
        if tracking_id.is_empty() {
            continue;
        }
        let mut discount_usd = get_cell(row, map, "Discount (USD)");
        if !discount_usd.is_empty() {
            if let Ok(v) = discount_usd.trim().replace(',', "").parse::<f64>() {
                if v > 0.0 {
                    discount_usd = format!("-{}", v);
                }
            }
        }
        let tax_exclusive_total_charge_usd = get_cell(row, map, "Tax Exclusive Total Charge (Base Rate - Discount + DAS + Fuel Surcharge + Additional Handling Fee + Peak Demand Surcharge + Non Standard Fee + Pickup Charge) (USD)");
        let tax_exclusive_total_charge_usd = if tax_exclusive_total_charge_usd.is_empty() {
            get_cell(row, map, "Tax Exclusive Charge Value (USD)")
        } else {
            tax_exclusive_total_charge_usd
        };
        let tax_inclusive_total_charge_usd = get_cell(row, map, "Tax Inclusive Total Charge (USD)");
        let tax_inclusive_total_charge_usd = if tax_inclusive_total_charge_usd.is_empty() {
            get_cell(row, map, "Tax Inclusive Charge Value (USD)")
        } else {
            tax_inclusive_total_charge_usd
        };
        items.push(AmazonInvoiceItem {
            tracking_id,
            order_id: get_cell_alt(row, map, "Order ID", "Order Id"),
            reference: get_cell(row, map, "Reference"),
            from_place: get_cell(row, map, "From"),
            from_zip: get_cell(row, map, "From Zip"),
            to_place: get_cell(row, map, "To"),
            to_zip: get_cell(row, map, "To Zip"),
            date_label_printed: get_cell(row, map, "Date Label Printed"),
            shipping_service: get_cell_alt(row, map, "Shipping service", "Shipping Service"),
            date_of_charge: get_cell(row, map, "Date of Charge"),
            leg_type: get_cell(row, map, "Leg Type"),
            comments: get_cell(row, map, "Comments"),
            child_account_id: get_cell(row, map, "Child Account Id"),
            zone: get_cell(row, map, "Zone"),
            base_rate_usd: get_cell_alt(row, map, "Base Rate (USD)", "Tax Exclusive Charge Value (USD)"),
            discount_usd,
            delivery_area_surcharge_usd: get_cell(row, map, "Delivery Area Surcharge(DAS) (USD)"),
            fuel_surcharge_usd: get_cell(row, map, "Fuel Surcharge (USD)"),
            additional_handeling_fee_usd: get_cell(row, map, "Additional Handling Fee (USD)"),
            peak_demand_surcharge_usd: get_cell(row, map, "Peak Demand Surcharge (USD)"),
            non_standard_fee_usd: get_cell(row, map, "Non Standard Fee (USD)"),
            pickup_charge_usd: get_cell(row, map, "Pickup Charge (USD)"),
            tax_exclusive_total_charge_usd,
            tax_amount_usd: get_cell(row, map, "Tax Amount (USD)"),
            tax_inclusive_total_charge_usd,
            billable_weight: get_cell_alt(row, map, "Billable Weight", "Billable Weight (lb)"),
            actual_weight: get_cell_alt(row, map, "Actual Weight", "Actual Weight (lb)"),
            dimensional_divisor: get_cell(row, map, "Dimensional Divisor"),
            length_in: get_cell_alt(row, map, "Length", "Length (in)"),
            width_in: get_cell_alt(row, map, "Width", "Width (in)"),
            height_in: get_cell_alt(row, map, "Height", "Height (in)"),
        });
    }
    Ok(items)
}

fn parse_items_v2(range: &calamine::Range<Data>, header_row_index: usize, map: &HeaderMap) -> Result<Vec<AmazonInvoiceItemV2>, String> {
    let mut rows = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        let tracking_id = get_cell(row, map, "Tracking Id");
        if tracking_id.is_empty() {
            continue;
        }
        rows.push(AmazonInvoiceItemV2 {
            child_account_id: get_cell(row, map, "Child Account Id"),
            tracking_id: tracking_id.clone(),
            order_id: get_cell(row, map, "Order Id"),
            reference: get_cell(row, map, "Reference"),
            from_place: get_cell(row, map, "From"),
            from_zip: get_cell(row, map, "From Zip"),
            to_place: get_cell(row, map, "To"),
            to_zip: get_cell(row, map, "To Zip"),
            date_label_printed: get_cell(row, map, "Date Label Printed"),
            date_of_charge: get_cell(row, map, "Date of Charge"),
            shipping_service: get_cell(row, map, "Shipping Service"),
            leg_type: get_cell(row, map, "Leg Type"),
            charge_type: get_cell(row, map, "Charge Type"),
            charge_code: get_cell(row, map, "Charge Code"),
            comments: get_cell(row, map, "Comments"),
            tax_exclusive_charge_value_usd: get_cell(row, map, "Tax Exclusive Charge Value (USD)"),
            tax_amount_usd: get_cell(row, map, "Tax Amount (USD)"),
            tax_inclusive_charge_value_usd: get_cell(row, map, "Tax Inclusive Charge Value (USD)"),
            zone: get_cell(row, map, "Zone"),
            billable_weight: get_cell(row, map, "Billable Weight (lb)"),
            actual_weight: get_cell(row, map, "Actual Weight (lb)"),
            dimensional_divisor: get_cell(row, map, "Dimensional Divisor"),
            length_in: get_cell(row, map, "Length (in)"),
            width_in: get_cell(row, map, "Width (in)"),
            height_in: get_cell(row, map, "Height (in)"),
        });
    }
    Ok(rows)
}

fn parse_adjustments_v1(
    workbook: &mut Xlsx<std::io::BufReader<std::fs::File>>,
) -> Result<Vec<AmazonInvoiceAdjustment>, String> {
    let sheet_names = workbook.sheet_names().clone();
    if sheet_names.len() < 2 {
        return Ok(Vec::new());
    }
    let range: calamine::Range<Data> = workbook
        .worksheet_range(&sheet_names[1])
        .map_err(|e: calamine::XlsxError| e.to_string())?;

    let header_row_index = 4usize;
    let header_row = match range.rows().nth(header_row_index) {
        Some(r) => r,
        None => return Ok(Vec::new()),
    };
    let header_map = build_header_map(header_row);

    let mut items = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        let tracking_id = get_cell(row, &header_map, "Tracking ID");
        let order_id = get_cell(row, &header_map, "Order ID");
        if tracking_id.is_empty() && order_id.is_empty() {
            continue;
        }
        items.push(AmazonInvoiceAdjustment {
            tracking_id,
            order_id,
            reference: get_cell(row, &header_map, "Reference"),
            date_label_printed: get_cell(row, &header_map, "Date Label Printed"),
            shipping_service: get_cell(row, &header_map, "Shipping Service"),
            date_of_charge: get_cell(row, &header_map, "Date of Charge"),
            charge_type: get_cell(row, &header_map, "Charge Type"),
            comments: get_cell(row, &header_map, "Comments"),
            base_rate_usd: get_cell(row, &header_map, "Base Rate  (USD)"),
            tax_amount_usd: get_cell(row, &header_map, "Tax Amount (USD)"),
            base_rate_incl_tax_usd: get_cell(
                row,
                &header_map,
                "Base Rate Including Tax Amount (USD)",
            ),
        });
    }
    Ok(items)
}

fn extract_adjustments_from_v2(items_v2: &[AmazonInvoiceItemV2]) -> Vec<AmazonInvoiceAdjustment> {
    items_v2
        .iter()
        .filter(|r| r.charge_type.trim().eq_ignore_ascii_case("Shipping Charge Adjustment"))
        .map(|r| AmazonInvoiceAdjustment {
            tracking_id: r.tracking_id.clone(),
            order_id: r.order_id.clone(),
            reference: r.reference.clone(),
            date_label_printed: r.date_label_printed.clone(),
            shipping_service: r.shipping_service.clone(),
            date_of_charge: r.date_of_charge.clone(),
            charge_type: r.charge_type.clone(),
            comments: r.comments.clone(),
            base_rate_usd: r.tax_inclusive_charge_value_usd.clone(),
            tax_amount_usd: r.tax_amount_usd.clone(),
            base_rate_incl_tax_usd: r.tax_inclusive_charge_value_usd.clone(),
        })
        .collect()
}

fn map_charge_to_column(
    charge_type: &str,
    charge_code: &str,
    tax_exclusive: &str,
    tax_amount: &str,
    tax_inclusive: &str,
    item: &mut AmazonInvoiceItem,
) -> bool {
    let ct = charge_type.trim();
    let cc = charge_code.trim();
    let eq_ignore = |a: &str, b: &str| a.eq_ignore_ascii_case(b);

    if eq_ignore(cc, "Base Charge") {
        if !tax_exclusive.is_empty() {
            item.base_rate_usd = tax_exclusive.to_string();
        }
        return true;
    }
    if eq_ignore(ct, "Discount") {
        if !tax_exclusive.is_empty() {
            item.discount_usd = tax_exclusive.to_string();
        }
        return true;
    }
    if eq_ignore(cc, "Delivery Area Surcharge") {
        if !tax_exclusive.is_empty() {
            item.delivery_area_surcharge_usd = tax_exclusive.to_string();
        }
        return true;
    }
    if eq_ignore(cc, "Fuel Surcharge") {
        if !tax_exclusive.is_empty() {
            item.fuel_surcharge_usd = tax_exclusive.to_string();
        }
        return true;
    }
    if eq_ignore(cc, "Demand Surcharge") {
        if !tax_exclusive.is_empty() {
            item.peak_demand_surcharge_usd = tax_exclusive.to_string();
        }
        return true;
    }
    if eq_ignore(cc, "Pickup Charge") {
        if !tax_exclusive.is_empty() {
            item.pickup_charge_usd = tax_exclusive.to_string();
        }
        return true;
    }
    if eq_ignore(ct, "Total") {
        if !tax_exclusive.is_empty() {
            item.tax_exclusive_total_charge_usd = tax_exclusive.to_string();
        }
        if !tax_amount.is_empty() {
            item.tax_amount_usd = tax_amount.to_string();
        }
        if !tax_inclusive.is_empty() {
            item.tax_inclusive_total_charge_usd = tax_inclusive.to_string();
        }
        return true;
    }
    false
}

fn v2_to_item_base(first: &AmazonInvoiceItemV2) -> AmazonInvoiceItem {
    AmazonInvoiceItem {
        tracking_id: first.tracking_id.clone(),
        order_id: first.order_id.clone(),
        reference: first.reference.clone(),
        from_place: first.from_place.clone(),
        from_zip: first.from_zip.clone(),
        to_place: first.to_place.clone(),
        to_zip: first.to_zip.clone(),
        date_label_printed: first.date_label_printed.clone(),
        shipping_service: first.shipping_service.clone(),
        date_of_charge: first.date_of_charge.clone(),
        leg_type: first.leg_type.clone(),
        comments: first.comments.clone(),
        child_account_id: first.child_account_id.clone(),
        zone: first.zone.clone(),
        base_rate_usd: String::new(),
        discount_usd: String::new(),
        delivery_area_surcharge_usd: String::new(),
        fuel_surcharge_usd: String::new(),
        additional_handeling_fee_usd: String::new(),
        peak_demand_surcharge_usd: String::new(),
        non_standard_fee_usd: String::new(),
        pickup_charge_usd: String::new(),
        tax_exclusive_total_charge_usd: String::new(),
        tax_amount_usd: String::new(),
        tax_inclusive_total_charge_usd: String::new(),
        billable_weight: first.billable_weight.clone(),
        actual_weight: first.actual_weight.clone(),
        dimensional_divisor: first.dimensional_divisor.clone(),
        length_in: first.length_in.clone(),
        width_in: first.width_in.clone(),
        height_in: first.height_in.clone(),
    }
}

fn convert_v2_to_v1(items_v2: Vec<AmazonInvoiceItemV2>) -> Result<Vec<AmazonInvoiceItem>, String> {
    let mut by_tracking: HashMap<String, Vec<AmazonInvoiceItemV2>> = HashMap::new();
    for v in items_v2 {
        by_tracking.entry(v.tracking_id.clone()).or_default().push(v);
    }
    let mut unmapped: HashMap<String, HashSet<String>> = HashMap::new();
    let mut result = Vec::new();
    for (_tid, group) in by_tracking {
        let first = &group[0];
        let mut item = v2_to_item_base(first);
        for row in &group {
            let ct = row.charge_type.trim();
            let cc = row.charge_code.trim();
            if ct.is_empty() && cc.is_empty() {
                continue;
            }
            if ct.eq_ignore_ascii_case("Shipping Charge Adjustment") {
                continue;
            }
            if !map_charge_to_column(
                &row.charge_type,
                &row.charge_code,
                &row.tax_exclusive_charge_value_usd,
                &row.tax_amount_usd,
                &row.tax_inclusive_charge_value_usd,
                &mut item,
            ) {
                unmapped
                    .entry(format!("Charge Type: '{}', Charge Code: '{}'", ct, cc))
                    .or_default()
                    .insert(item.tracking_id.clone());
            }
        }
        result.push(item);
    }
    if unmapped.is_empty() {
        Ok(result)
    } else {
        let details: Vec<String> = unmapped
            .iter()
            .map(|(k, v)| {
                let ex: String = v.iter().take(3).map(String::as_str).collect::<Vec<_>>().join(", ");
                let n = v.len();
                if n > 3 {
                    format!("{} - Example Tracking Ids: {} (and {} more)", k, ex, n - 3)
                } else if n > 1 {
                    format!("{} - Tracking Ids: {}", k, ex)
                } else {
                    format!("{} - Tracking Id: {}", k, ex)
                }
            })
            .collect();
        Err(format!("Excel format not supported. Unmapped Charge Type/Code found:\n{}", details.join("\n")))
    }
}

fn is_new_version(headers: &[String]) -> bool {
    let set: HashSet<String> = headers.iter().map(|s| s.to_lowercase()).collect();
    set.contains("child account id") || set.contains("charge type")
}

fn normalize_column_name(input: &str) -> String {
    let s = input.replace("\r\n", " ").replace('\n', " ").replace('\r', " ");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_header_detailed(headers: &[String]) -> (bool, Option<String>) {
    let old_set: HashSet<String> = [
        "Tracking Id", "Order ID", "Reference", "Date Label Printed", "Shipping service",
        "Date of Charge", "Zone", "Base Rate (USD)", "Discount (USD)",
        "Delivery Area Surcharge(DAS) (USD)", "Fuel Surcharge (USD)", "Additional Handling Fee (USD)",
        "Peak Demand Surcharge (USD)", "Non Standard Fee (USD)", "Pickup Charge (USD)",
        "Tax Exclusive Total Charge (Base Rate - Discount + DAS + Fuel Surcharge + Additional Handling Fee + Peak Demand Surcharge + Non Standard Fee + Pickup Charge) (USD)",
        "Tax Amount (USD)", "Tax Inclusive Total Charge (USD)", "Billable Weight", "Actual Weight",
        "Dimensional Divisor", "Length", "Width", "Height",
    ]
    .iter()
    .map(|s| s.to_lowercase())
    .collect();
    let new_set: HashSet<String> = [
        "Child Account Id", "Tracking Id", "Order Id", "Reference", "From", "From Zip", "To", "To Zip",
        "Date Label Printed", "Date of Charge", "Shipping Service", "Leg Type", "Charge Type", "Charge Code", "Comments",
        "Tax Exclusive Charge Value (USD)", "Tax Amount (USD)", "Tax Inclusive Charge Value (USD)", "Zone",
        "Billable Weight (lb)", "Actual Weight (lb)", "Dimensional Divisor", "Length (in)", "Width (in)", "Height (in)",
    ]
    .iter()
    .map(|s| s.to_lowercase())
    .collect();
    let expected = if is_new_version(headers) { &new_set } else { &old_set };
    let unexpected: Vec<String> = headers
        .iter()
        .filter(|h| !expected.contains(&h.to_lowercase()))
        .cloned()
        .collect();
    if unexpected.is_empty() {
        (true, None)
    } else {
        (false, Some(format!("Excel format not supported. Unexpected columns found: {}", unexpected.join(", "))))
    }
}

