//! Temu 承运商：表头第 0 行，ReadByHeader<TemuInvoice>，无发票号校验，直接插入 shipping_invoice_temu

use crate::invoice::{CarrierStrategy, FileInfoDto};
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::collections::HashMap;
use std::path::Path;

pub struct TemuStrategy;

impl TemuStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
pub struct TemuInvoice {
    pub transaction_date: String,
    pub transaction_type: String,
    pub related_id: String,
    pub order_id: String,
    pub order_item_id: String,
    pub sku: String,
    pub sku_id: String,
    pub quantity: String,
    pub ship_city: String,
    pub ship_state: String,
    pub retail_price: String,
    pub platform_discount: String,
    pub seller_discount: String,
    pub service_fee: String,
    pub service_fee_tax: String,
    pub platform_incentive: String,
    pub subtotal: String,
    pub shipping: String,
    pub platform_incentive_shipping: String,
    pub product_tax: String,
    pub shipping_tax: String,
    pub marketplace_withheld_tax: String,
    pub others: String,
    pub total: String,
    pub currency: String,
    pub upload_invoice_file_name: String,
    pub upload_invoice_file_date: String,
}

type HeaderMap = HashMap<String, usize>;

impl CarrierStrategy for TemuStrategy {
    type ParsedData = Vec<TemuInvoice>;

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
        let items = parse_items(&range, header_row_index, &header_map)?;
        if items.is_empty() {
            return Err("No records found in Excel.".to_string());
        }

        let mut items = items;
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for inv in &mut items {
            inv.upload_invoice_file_name = file_name.clone();
            inv.upload_invoice_file_date = now.clone();
        }

        let file_info = FileInfoDto {
            file_full_name: path.to_string(),
            file_name,
            status: "Init".to_string(),
            row_count: items.len() as i32,
            carrier: "Temu".to_string(),
            account_number: None,
            invoice_number: None,
        };

        Ok((file_info, items))
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
        "transaction-type",
        "related-id",
        "order id",
        "order item id",
        "sku",
        "total",
    ];
    let headers: std::collections::HashSet<String> = header_row
        .iter()
        .map(|c| c.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    required.iter().all(|h| headers.contains(*h))
}

fn parse_items(
    range: &calamine::Range<Data>,
    header_row_index: usize,
    map: &HeaderMap,
) -> Result<Vec<TemuInvoice>, String> {
    let mut items = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        let transaction_type = get_cell(row, map, "transaction-type");
        if transaction_type.is_empty() && get_cell(row, map, "order id").is_empty() {
            continue;
        }
        items.push(TemuInvoice {
            transaction_date: get_cell(row, map, "date/time"),
            transaction_type,
            related_id: get_cell(row, map, "related-id"),
            order_id: get_cell(row, map, "order id"),
            order_item_id: get_cell(row, map, "order item id"),
            sku: get_cell(row, map, "sku"),
            sku_id: get_cell(row, map, "sku id"),
            quantity: get_cell(row, map, "quantity"),
            ship_city: get_cell(row, map, "ship city"),
            ship_state: get_cell(row, map, "ship state"),
            retail_price: get_cell(row, map, "retail price"),
            platform_discount: get_cell(row, map, "platform discount"),
            seller_discount: get_cell(row, map, "seller discount"),
            service_fee: get_cell(row, map, "service fee"),
            service_fee_tax: get_cell(row, map, "service fee tax"),
            platform_incentive: get_cell(row, map, "platform incentive"),
            subtotal: get_cell(row, map, "subtotal"),
            shipping: get_cell(row, map, "shipping"),
            platform_incentive_shipping: get_cell(row, map, "platform incentive - shipping"),
            product_tax: get_cell(row, map, "product tax"),
            shipping_tax: get_cell(row, map, "shipping tax"),
            marketplace_withheld_tax: get_cell(row, map, "marketplace withheld tax"),
            others: get_cell(row, map, "others"),
            total: get_cell(row, map, "total"),
            currency: get_cell(row, map, "currency"),
            upload_invoice_file_name: String::new(),
            upload_invoice_file_date: String::new(),
        });
    }
    Ok(items)
}
