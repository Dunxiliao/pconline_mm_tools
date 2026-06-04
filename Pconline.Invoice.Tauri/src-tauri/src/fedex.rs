//! Fedex 承运商：表头第 0 行，ReadByHeader<FedexInvoice> + ProcessCharges，校验 invoice_number

use crate::invoice::{CarrierStrategy, FileInfoDto};
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct FedexStrategy;

impl FedexStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
pub struct FedexInvoice {
    pub consolidated_account: String,
    pub bill_to_account_number: String,
    pub invoice_date: String,
    pub invoice_number: String,
    pub store_id: String,
    pub original_amount_due: String,
    pub current_balance: String,
    pub payor: String,
    pub ground_tracking_id_prefix: String,
    pub express_or_ground_tracking_id: String,
    pub transportation_charge_amount: String,
    pub net_charge_amount: String,
    pub service_type: String,
    pub ground_service: String,
    pub shipment_date: String,
    pub pod_delivery_date: String,
    pub pod_delivery_time: String,
    pub pod_service_area_code: String,
    pub pod_signature_description: String,
    pub actual_weight_amount: String,
    pub actual_weight_units: String,
    pub rated_weight_amount: String,
    pub rated_weight_units: String,
    pub number_of_pieces: String,
    pub bundle_number: String,
    pub meter_number: String,
    pub tdmastertrackingid: String,
    pub service_packaging: String,
    pub dim_length: String,
    pub dim_width: String,
    pub dim_height: String,
    pub dim_divisor: String,
    pub dim_unit: String,
    pub recipient_name: String,
    pub recipient_company: String,
    pub recipient_address_line_1: String,
    pub recipient_address_line_2: String,
    pub recipient_city: String,
    pub recipient_state: String,
    pub recipient_zip_code: String,
    pub recipient_country_or_territory: String,
    pub shipper_company: String,
    pub shipper_name: String,
    pub shipper_address_line_1: String,
    pub shipper_address_line_2: String,
    pub shipper_city: String,
    pub shipper_state: String,
    pub shipper_zip_code: String,
    pub shipper_country_or_territory: String,
    pub original_customer_reference: String,
    pub original_ref_2: String,
    pub original_ref_3_or_po_number: String,
    pub original_department_reference_description: String,
    pub updated_customer_reference: String,
    pub updated_ref_2: String,
    pub updated_ref_3_or_po_number: String,
    pub updated_department_reference_description: String,
    pub rma_: String,
    pub original_recipient_address_line_1: String,
    pub original_recipient_address_line_2: String,
    pub original_recipient_city: String,
    pub original_recipient_state: String,
    pub original_recipient_zip_code: String,
    pub original_recipient_country_or_territory: String,
    pub zone_code: String,
    pub cost_allocation: String,
    pub alternate_address_line_1: String,
    pub alternate_address_line_2: String,
    pub alternate_city: String,
    pub alternate_state_province: String,
    pub alternate_zip_code: String,
    pub alternate_country_or_territory_code: String,
    pub crossreftrackingid_prefix: String,
    pub crossreftrackingid: String,
    pub entry_date: String,
    pub entry_number: String,
    pub customs_value: String,
    pub customs_value_currency_code: String,
    pub declared_value: String,
    pub declared_value_currency_code: String,
    pub commodity_description: String,
    pub commodity_country_or_territory_code: String,
    pub commodity_description_1: String,
    pub commodity_country_or_territory_code_1: String,
    pub commodity_description_2: String,
    pub commodity_country_or_territory_code_2: String,
    pub commodity_description_3: String,
    pub commodity_country_or_territory_code_3: String,
    pub currency_conversion_date: String,
    pub currency_conversion_rate: String,
    pub multiweight_number: String,
    pub multiweight_total_multiweight_units: String,
    pub multiweight_total_multiweight_weight: String,
    pub multiweight_total_shipment_charge_amount: String,
    pub multiweight_total_shipment_weight: String,
    pub ground_tracking_id_address_correction_discount_charge_amount: String,
    pub ground_tracking_id_address_correction_gross_charge_amount: String,
    pub rated_method: String,
    pub sort_hub: String,
    pub estimated_weight: String,
    pub estimated_weight_unit: String,
    pub postal_class: String,
    pub process_category: String,
    pub package_size: String,
    pub delivery_confirmation: String,
    pub tendered_date: String,
    pub mps_package_id: String,
    pub shipment_notes: String,
    pub upload_invoice_file_name: String,
    pub upload_invoice_file_date: String,
}

#[derive(Debug, Clone)]
pub struct FedexCharge {
    pub express_or_ground_tracking_id: String,
    pub charge_type: String,
    pub charge_amount: String,
    pub upload_invoice_file_name: String,
    #[allow(dead_code)]
    pub upload_invoice_file_date: String,
}

#[derive(Debug, Clone)]
pub struct FedexInfo {
    pub invoices: Vec<FedexInvoice>,
    pub charges: Vec<FedexCharge>,
}

type HeaderMap = HashMap<String, usize>;

impl CarrierStrategy for FedexStrategy {
    type ParsedData = FedexInfo;

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
            return Err("File format does not match the selected carrier. Please double check your file and upload again.".to_string());
        }

        let header_map = build_header_map(header_row);
        let mut invoices = parse_invoices(&range, header_row_index, &header_map)?;
        if invoices.is_empty() {
            return Err("No records found in Excel.".to_string());
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for inv in &mut invoices {
            inv.upload_invoice_file_name = file_name.clone();
            inv.upload_invoice_file_date = now.clone();
        }

        let charges = process_charges(&range, header_row_index, header_row, &file_name, &now);

        let mut seen_acc: HashSet<String> = HashSet::new();
        let mut ordered_acc: Vec<String> = Vec::new();
        for i in &invoices {
            if seen_acc.insert(i.bill_to_account_number.clone()) {
                ordered_acc.push(i.bill_to_account_number.clone());
            }
        }
        let mut seen_inv: HashSet<String> = HashSet::new();
        let mut ordered_inv: Vec<String> = Vec::new();
        for i in &invoices {
            if seen_inv.insert(i.invoice_number.clone()) {
                ordered_inv.push(i.invoice_number.clone());
            }
        }
        let account_number: String = ordered_acc.join(",");
        let invoice_number: String = ordered_inv.join("\n");

        let file_info = FileInfoDto {
            file_full_name: path.to_string(),
            file_name,
            status: "Init".to_string(),
            row_count: invoices.len() as i32,
            carrier: "Fedex".to_string(),
            account_number: Some(account_number).filter(|s| !s.is_empty()),
            invoice_number: Some(invoice_number).filter(|s| !s.is_empty()),
        };

        Ok((file_info, FedexInfo { invoices, charges }))
    }
}

fn normalize_column_name(input: &str) -> String {
    let s = input.replace("\r\n", " ").replace('\n', " ").replace('\r', " ");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
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

fn validate_header(header_row: &[calamine::Data]) -> bool {
    let required = [
        "Consolidated Account",
        "Bill to Account Number",
        "Invoice Date",
        "Invoice Number",
        "Original Amount Due",
        "Express or Ground Tracking ID",
    ];
    let headers: std::collections::HashSet<String> = header_row
        .iter()
        .map(|c| c.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    required.iter().all(|h| headers.contains(*h))
}

fn parse_invoices(
    range: &calamine::Range<Data>,
    header_row_index: usize,
    map: &HeaderMap,
) -> Result<Vec<FedexInvoice>, String> {
    let mut items = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        let tracking_id = get_cell(row, map, "Express or Ground Tracking ID");
        let invoice_number = get_cell(row, map, "Invoice Number");
        if tracking_id.is_empty() && invoice_number.is_empty() {
            continue;
        }
        items.push(FedexInvoice {
            consolidated_account: get_cell(row, map, "Consolidated Account"),
            bill_to_account_number: get_cell(row, map, "Bill to Account Number"),
            invoice_date: get_cell(row, map, "Invoice Date"),
            invoice_number,
            store_id: get_cell(row, map, "Store ID"),
            original_amount_due: get_cell(row, map, "Original Amount Due"),
            current_balance: get_cell(row, map, "Current Balance"),
            payor: get_cell(row, map, "Payor"),
            ground_tracking_id_prefix: get_cell(row, map, "Ground Tracking ID Prefix"),
            express_or_ground_tracking_id: tracking_id,
            transportation_charge_amount: get_cell(row, map, "Transportation Charge Amount"),
            net_charge_amount: get_cell(row, map, "Net Charge Amount"),
            service_type: get_cell(row, map, "Service Type"),
            ground_service: get_cell(row, map, "Ground Service"),
            shipment_date: get_cell(row, map, "Shipment Date"),
            pod_delivery_date: get_cell(row, map, "POD Delivery Date"),
            pod_delivery_time: get_cell(row, map, "POD Delivery Time"),
            pod_service_area_code: get_cell(row, map, "POD Service Area Code"),
            pod_signature_description: get_cell(row, map, "POD Signature Description"),
            actual_weight_amount: get_cell(row, map, "Actual Weight Amount"),
            actual_weight_units: get_cell(row, map, "Actual Weight Units"),
            rated_weight_amount: get_cell(row, map, "Rated Weight Amount"),
            rated_weight_units: get_cell(row, map, "Rated Weight Units"),
            number_of_pieces: get_cell(row, map, "Number of Pieces"),
            bundle_number: get_cell(row, map, "Bundle Number"),
            meter_number: get_cell(row, map, "Meter Number"),
            tdmastertrackingid: get_cell(row, map, "TDMasterTrackingID"),
            service_packaging: get_cell(row, map, "Service Packaging"),
            dim_length: get_cell(row, map, "Dim Length"),
            dim_width: get_cell(row, map, "Dim Width"),
            dim_height: get_cell(row, map, "Dim Height"),
            dim_divisor: get_cell(row, map, "Dim Divisor"),
            dim_unit: get_cell(row, map, "Dim Unit"),
            recipient_name: get_cell(row, map, "Recipient Name"),
            recipient_company: get_cell(row, map, "Recipient Company"),
            recipient_address_line_1: get_cell(row, map, "Recipient Address Line 1"),
            recipient_address_line_2: get_cell(row, map, "Recipient Address Line 2"),
            recipient_city: get_cell(row, map, "Recipient City"),
            recipient_state: get_cell(row, map, "Recipient State"),
            recipient_zip_code: get_cell(row, map, "Recipient Zip Code"),
            recipient_country_or_territory: get_cell(row, map, "Recipient Country/Territory"),
            shipper_company: get_cell(row, map, "Shipper Company"),
            shipper_name: get_cell(row, map, "Shipper Name"),
            shipper_address_line_1: get_cell(row, map, "Shipper Address Line 1"),
            shipper_address_line_2: get_cell(row, map, "Shipper Address Line 2"),
            shipper_city: get_cell(row, map, "Shipper City"),
            shipper_state: get_cell(row, map, "Shipper State"),
            shipper_zip_code: get_cell(row, map, "Shipper Zip Code"),
            shipper_country_or_territory: get_cell(row, map, "Shipper Country/Territory"),
            original_customer_reference: get_cell(row, map, "Original Customer Reference"),
            original_ref_2: get_cell(row, map, "Original Ref#2"),
            original_ref_3_or_po_number: get_cell(row, map, "Original Ref#3/PO Number"),
            original_department_reference_description: get_cell(
                row,
                map,
                "Original Department Reference Description",
            ),
            updated_customer_reference: get_cell(row, map, "Updated Customer Reference"),
            updated_ref_2: get_cell(row, map, "Updated Ref#2"),
            updated_ref_3_or_po_number: get_cell(row, map, "Updated Ref#3/PO Number"),
            updated_department_reference_description: get_cell(
                row,
                map,
                "Updated Department Reference Description",
            ),
            rma_: get_cell(row, map, "RMA#"),
            original_recipient_address_line_1: get_cell(
                row,
                map,
                "Original Recipient Address Line 1",
            ),
            original_recipient_address_line_2: get_cell(
                row,
                map,
                "Original Recipient Address Line 2",
            ),
            original_recipient_city: get_cell(row, map, "Original Recipient City"),
            original_recipient_state: get_cell(row, map, "Original Recipient State"),
            original_recipient_zip_code: get_cell(row, map, "Original Recipient Zip Code"),
            original_recipient_country_or_territory: get_cell(
                row,
                map,
                "Original Recipient Country/Territory",
            ),
            zone_code: get_cell(row, map, "Zone Code"),
            cost_allocation: get_cell(row, map, "Cost Allocation"),
            alternate_address_line_1: get_cell(row, map, "Alternate Address Line 1"),
            alternate_address_line_2: get_cell(row, map, "Alternate Address Line 2"),
            alternate_city: get_cell(row, map, "Alternate City"),
            alternate_state_province: get_cell(row, map, "Alternate State Province"),
            alternate_zip_code: get_cell(row, map, "Alternate Zip Code"),
            alternate_country_or_territory_code: get_cell(
                row,
                map,
                "Alternate Country/Territory Code",
            ),
            crossreftrackingid_prefix: get_cell(row, map, "CrossRefTrackingID Prefix"),
            crossreftrackingid: get_cell(row, map, "CrossRefTrackingID"),
            entry_date: get_cell(row, map, "Entry Date"),
            entry_number: get_cell(row, map, "Entry Number"),
            customs_value: get_cell(row, map, "Customs Value"),
            customs_value_currency_code: get_cell(row, map, "Customs Value Currency Code"),
            declared_value: get_cell(row, map, "Declared Value"),
            declared_value_currency_code: get_cell(row, map, "Declared Value Currency Code"),
            commodity_description: get_cell(row, map, "Commodity Description"),
            commodity_country_or_territory_code: get_cell(
                row,
                map,
                "Commodity Country/Territory Code",
            ),
            commodity_description_1: String::new(),
            commodity_country_or_territory_code_1: String::new(),
            commodity_description_2: String::new(),
            commodity_country_or_territory_code_2: String::new(),
            commodity_description_3: String::new(),
            commodity_country_or_territory_code_3: String::new(),
            currency_conversion_date: get_cell(row, map, "Currency Conversion Date"),
            currency_conversion_rate: get_cell(row, map, "Currency Conversion Rate"),
            multiweight_number: get_cell(row, map, "Multiweight Number"),
            multiweight_total_multiweight_units: get_cell(
                row,
                map,
                "Multiweight Total Multiweight Units",
            ),
            multiweight_total_multiweight_weight: get_cell(
                row,
                map,
                "Multiweight Total Multiweight Weight",
            ),
            multiweight_total_shipment_charge_amount: get_cell(
                row,
                map,
                "Multiweight Total Shipment Charge Amount",
            ),
            multiweight_total_shipment_weight: get_cell(
                row,
                map,
                "Multiweight Total Shipment Weight",
            ),
            ground_tracking_id_address_correction_discount_charge_amount: get_cell(
                row,
                map,
                "Ground Tracking ID Address Correction Discount Charge Amount",
            ),
            ground_tracking_id_address_correction_gross_charge_amount: get_cell(
                row,
                map,
                "Ground Tracking ID Address Correction Gross Charge Amount",
            ),
            rated_method: get_cell(row, map, "Rated Method"),
            sort_hub: get_cell(row, map, "Sort Hub"),
            estimated_weight: get_cell(row, map, "Estimated Weight"),
            estimated_weight_unit: get_cell(row, map, "Estimated Weight Unit"),
            postal_class: get_cell(row, map, "Postal Class"),
            process_category: get_cell(row, map, "Process Category"),
            package_size: get_cell(row, map, "Package Size"),
            delivery_confirmation: get_cell(row, map, "Delivery Confirmation"),
            tendered_date: get_cell(row, map, "Tendered Date"),
            mps_package_id: get_cell(row, map, "MPS Package ID"),
            shipment_notes: get_cell(row, map, "Shipment Notes"),
            upload_invoice_file_name: String::new(),
            upload_invoice_file_date: String::new(),
        });
    }
    Ok(items)
}

/// 找列名含 "tracking id charge description" 的列，取该列为 charge_type，下一列为 charge_amount
fn process_charges(
    range: &calamine::Range<Data>,
    header_row_index: usize,
    header_row: &[calamine::Data],
    file_name: &str,
    now: &str,
) -> Vec<FedexCharge> {
    let mut charge_cols = Vec::new();
    for (col, c) in header_row.iter().enumerate() {
        let h = normalize_column_name(&c.to_string()).to_lowercase();
        if h.contains("tracking id charge description") {
            charge_cols.push(col);
        }
    }
    let tracking_id_col = header_row
        .iter()
        .enumerate()
        .find(|(_, c)| {
            normalize_column_name(&c.to_string()).to_lowercase() == "express or ground tracking id"
        })
        .map(|(i, _)| i);

    let mut charges = Vec::new();
    let tracking_id_col = match tracking_id_col {
        Some(c) => c,
        None => return charges,
    };

    for row in range.rows().skip(header_row_index + 1) {
        let tracking_id = row
            .get(tracking_id_col)
            .map(|c| c.to_string().trim().to_string())
            .unwrap_or_default();
        if tracking_id.is_empty() {
            continue;
        }
        for &col in &charge_cols {
            let charge_type = row.get(col).map(|c| c.to_string().trim().to_string()).unwrap_or_default();
            let charge_amount = row.get(col + 1).map(|c| c.to_string().trim().to_string()).unwrap_or_default();
            if !charge_type.is_empty() && !charge_amount.is_empty() {
                charges.push(FedexCharge {
                    express_or_ground_tracking_id: tracking_id.clone(),
                    charge_type,
                    charge_amount,
                    upload_invoice_file_name: file_name.to_string(),
                    upload_invoice_file_date: now.to_string(),
                });
            }
        }
    }
    charges
}
