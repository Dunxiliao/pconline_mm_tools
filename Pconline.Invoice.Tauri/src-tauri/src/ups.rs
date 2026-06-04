//! UPS 承运商：表头第 0 行，ReadByHeader<UpsInvoice>，RestockInvoices 补全空运单号，校验 invoicenum

use crate::invoice::{CarrierStrategy, FileInfoDto};
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::collections::HashMap;
use std::path::Path;

pub struct UpsStrategy;

impl UpsStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone)]
pub struct UpsInvoice {
    pub track_num: String,
    pub track_num_alt: String,
    pub rpt: String,
    pub source_area: String,
    pub service_text: String,
    pub act_num: String,
    pub act_num_shipped: String,
    pub act_num_billed: String,
    pub invdate: String,
    pub pudate: String,
    pub inout: String,
    pub mvmt: String,
    pub chgtyp: String,
    pub pdt: String,
    pub ctn: String,
    pub svc: String,
    pub rc: String,
    pub spmt: String,
    pub spmt_num: String,
    pub billtyp: String,
    pub pkgs: String,
    pub cnt: String,
    pub zip: String,
    pub zone: String,
    pub act_wgt: String,
    pub wgt_audited: String,
    pub bill_wgt: String,
    pub wgt_uom: String,
    pub wgt_src: String,
    pub os: String,
    pub l: String,
    pub w: String,
    pub h: String,
    pub cubic: String,
    pub l_ke: String,
    pub w_ke: String,
    pub h_ke: String,
    pub dim_uom: String,
    pub r#pub: String,
    pub inc: String,
    pub net: String,
    pub ahc_t: String,
    pub bop_t: String,
    pub das_t: String,
    pub dc_t: String,
    pub di_t: String,
    pub haz_t: String,
    pub ins_t: String,
    pub ins_units: String,
    pub declared_value: String,
    pub lps_t: String,
    pub oms_t: String,
    pub opu_t: String,
    pub pas_t: String,
    pub pdl_t: String,
    pub ppu_t: String,
    pub psc_t: String,
    pub rtn_t: String,
    pub adc: String,
    pub ahc: String,
    pub sah: String,
    pub bop: String,
    pub cbf: String,
    pub cns: String,
    pub cnv: String,
    pub cod: String,
    pub coo: String,
    pub das: String,
    pub dc: String,
    pub ddo: String,
    pub di: String,
    pub dtr: String,
    pub faf: String,
    pub ftp: String,
    pub fts: String,
    pub haz: String,
    pub ins: String,
    pub loc: String,
    pub ish: String,
    pub lps: String,
    pub slp: String,
    pub lsc: String,
    pub oms: String,
    pub sov: String,
    pub opu: String,
    pub par: String,
    pub pas: String,
    pub pdl: String,
    pub ppu: String,
    pub psc: String,
    pub rep: String,
    pub res: String,
    pub rtn: String,
    pub sed: String,
    pub sur: String,
    pub oth: String,
    pub tax: String,
    pub gst: String,
    pub hst: String,
    pub pst: String,
    pub qst: String,
    pub gst_pct: String,
    pub hst_pct: String,
    pub pst_pct: String,
    pub qst_pct: String,
    pub fsc_pct: String,
    pub fsc_pub: String,
    pub fsc_inc: String,
    pub fsc_net: String,
    pub tot_acc: String,
    pub rca: String,
    pub scc: String,
    pub tot_pub: String,
    pub tot_inc: String,
    pub tot_net: String,
    pub tot_adj: String,
    pub currency: String,
    pub exchange_rate: String,
    pub exchange_from_to: String,
    pub rrdd: String,
    pub invoicenum: String,
    pub pur: String,
    pub bt: String,
    pub book: String,
    pub pg: String,
    pub shipmentid: String,
    pub ref1: String,
    pub ref2: String,
    pub ref3: String,
    pub userid: String,
    pub msg_codes: String,
    pub notes: String,
    pub port: String,
    pub s_cntry: String,
    pub r_cntry: String,
    pub tp_cntry: String,
    pub sender: String,
    pub s_name: String,
    pub s_company: String,
    pub s_address: String,
    pub s_city: String,
    pub s_state: String,
    pub s_zip: String,
    pub s_zip_corrected: String,
    pub receiver: String,
    pub r_name: String,
    pub r_company: String,
    pub r_address: String,
    pub r_city: String,
    pub r_state: String,
    pub r_zip: String,
    pub thirdparty: String,
    pub tp_name: String,
    pub tp_company: String,
    pub tp_address: String,
    pub tp_city: String,
    pub tp_state: String,
    pub tp_zip: String,
    pub orig_servicetext: String,
    pub orig_zip: String,
    pub orig_zone: String,
    pub orig_wgt: String,
    pub orig_pub: String,
    pub orig_inc: String,
    pub orig_net: String,
    pub new_wgt: String,
    pub new_pub: String,
    pub new_inc: String,
    pub new_net: String,
    pub daily_rate: String,
    pub tsc: String,
    pub tsvc: String,
    pub shipper_name: String,
    pub tfilter_ref: String,
    pub upload_invoice_file_name: String,
    pub upload_invoice_file_date: String,
}

type HeaderMap = HashMap<String, usize>;

impl CarrierStrategy for UpsStrategy {
    type ParsedData = Vec<UpsInvoice>;

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
        let mut items = parse_items(&range, header_row_index, &header_map)?;
        if items.is_empty() {
            return Err("No records found in Excel.".to_string());
        }

        items = restock_invoices(items);
        // 对齐 C#：DateTime.Now（本地时间）
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for inv in &mut items {
            inv.upload_invoice_file_name = file_name.clone();
            inv.upload_invoice_file_date = now.clone();
        }

        // 对齐 C#：Distinct() 保序去重
        let mut seen_act: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ordered_act: Vec<String> = Vec::new();
        for i in &items {
            if seen_act.insert(i.act_num.clone()) {
                ordered_act.push(i.act_num.clone());
            }
        }

        let mut seen_inv: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ordered_inv: Vec<String> = Vec::new();
        for i in &items {
            if seen_inv.insert(i.invoicenum.clone()) {
                ordered_inv.push(i.invoicenum.clone());
            }
        }

        let account_number: String = ordered_act.join(",");
        let invoice_number: String = ordered_inv.join("\n");

        let file_info = FileInfoDto {
            file_full_name: path.to_string(),
            file_name,
            status: "Init".to_string(),
            row_count: items.len() as i32,
            carrier: "UPS".to_string(),
            account_number: Some(account_number).filter(|s| !s.is_empty()),
            invoice_number: Some(invoice_number).filter(|s| !s.is_empty()),
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

fn normalize_excel_date_text(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }
    // 已是日期文本时直接返回
    if s.contains('-') || s.contains('/') || s.contains(':') {
        return s.to_string();
    }
    // 兼容 Excel 日期序列（与 C# DateUtil 语义一致，基准日 1899-12-30）
    if let Ok(serial) = s.parse::<f64>() {
        if (1.0..=60000.0).contains(&serial) {
            let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                .and_then(|d| d.and_hms_opt(0, 0, 0));
            if let Some(base) = base {
                let secs = (serial * 86400.0).round() as i64;
                let dt = base + chrono::Duration::seconds(secs);
                return dt.format("%Y/%m/%d %H:%M:%S").to_string();
            }
        }
    }
    s.to_string()
}

fn get_cell_date(row: &[calamine::Data], map: &HeaderMap, name: &str) -> String {
    normalize_excel_date_text(&get_cell(row, map, name))
}

fn validate_header(header_row: &[calamine::Data]) -> bool {
    // 对齐 C# UpsStrategy.ValidateHeader：
    // 直接 Trim 后做严格字符串匹配（大小写敏感，不 normalize）。
    let required = ["Track_Num", "Track_Num_Alt", "Act_Num", "Act_Num_Shipped", "InvoiceNum", "UserID"];
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
) -> Result<Vec<UpsInvoice>, String> {
    let mut items = Vec::new();
    for row in range.rows().skip(header_row_index + 1) {
        items.push(UpsInvoice {
            track_num: get_cell(row, map, "Track_Num"),
            track_num_alt: get_cell(row, map, "Track_Num_Alt"),
            rpt: get_cell(row, map, "Rpt"),
            source_area: get_cell(row, map, "Source_Area"),
            service_text: get_cell(row, map, "Service_Text"),
            act_num: get_cell(row, map, "Act_Num"),
            act_num_shipped: get_cell(row, map, "Act_Num_Shipped"),
            act_num_billed: get_cell(row, map, "Act_Num_Billed"),
            invdate: get_cell_date(row, map, "InvDate"),
            pudate: get_cell_date(row, map, "PUDate"),
            inout: get_cell(row, map, "InOut"),
            mvmt: get_cell(row, map, "MVMT"),
            chgtyp: get_cell(row, map, "ChgTyp"),
            pdt: get_cell(row, map, "PDT"),
            ctn: get_cell(row, map, "CTN"),
            svc: get_cell(row, map, "SVC"),
            rc: get_cell(row, map, "RC"),
            spmt: get_cell(row, map, "SPMT"),
            spmt_num: get_cell(row, map, "SPMT_Num"),
            billtyp: get_cell(row, map, "BillTyp"),
            pkgs: get_cell(row, map, "Pkgs"),
            cnt: get_cell(row, map, "CNT"),
            zip: get_cell(row, map, "Zip"),
            zone: get_cell(row, map, "Zone"),
            act_wgt: get_cell(row, map, "Act_Wgt"),
            wgt_audited: get_cell(row, map, "Wgt_Audited"),
            bill_wgt: get_cell(row, map, "Bill_Wgt"),
            wgt_uom: get_cell(row, map, "Wgt_UOM"),
            wgt_src: get_cell(row, map, "Wgt_Src"),
            os: get_cell(row, map, "OS"),
            l: get_cell(row, map, "L"),
            w: get_cell(row, map, "W"),
            h: get_cell(row, map, "H"),
            cubic: get_cell(row, map, "Cubic"),
            l_ke: get_cell(row, map, "L_KE"),
            w_ke: get_cell(row, map, "W_KE"),
            h_ke: get_cell(row, map, "H_KE"),
            dim_uom: get_cell(row, map, "Dim_UOM"),
            r#pub: get_cell(row, map, "Pub"),
            inc: get_cell(row, map, "Inc"),
            net: get_cell(row, map, "Net"),
            ahc_t: get_cell(row, map, "AHC_T"),
            bop_t: get_cell(row, map, "BOP_T"),
            das_t: get_cell(row, map, "DAS_T"),
            dc_t: get_cell(row, map, "DC_T"),
            di_t: get_cell(row, map, "DI_T"),
            haz_t: get_cell(row, map, "HAZ_T"),
            ins_t: get_cell(row, map, "INS_T"),
            ins_units: get_cell(row, map, "INS_UNITS"),
            declared_value: get_cell(row, map, "Declared_Value"),
            lps_t: get_cell(row, map, "LPS_T"),
            oms_t: get_cell(row, map, "OMS_T"),
            opu_t: get_cell(row, map, "OPU_T"),
            pas_t: get_cell(row, map, "PAS_T"),
            pdl_t: get_cell(row, map, "PDL_T"),
            ppu_t: get_cell(row, map, "PPU_T"),
            psc_t: get_cell(row, map, "PSC_T"),
            rtn_t: get_cell(row, map, "RTN_T"),
            adc: get_cell(row, map, "ADC"),
            ahc: get_cell(row, map, "AHC"),
            sah: get_cell(row, map, "SAH"),
            bop: get_cell(row, map, "BOP"),
            cbf: get_cell(row, map, "CBF"),
            cns: get_cell(row, map, "CNS"),
            cnv: get_cell(row, map, "CNV"),
            cod: get_cell(row, map, "COD"),
            coo: get_cell(row, map, "COO"),
            das: get_cell(row, map, "DAS"),
            dc: get_cell(row, map, "DC"),
            ddo: get_cell(row, map, "DDO"),
            di: get_cell(row, map, "DI"),
            dtr: get_cell(row, map, "DTR"),
            faf: get_cell(row, map, "FAF"),
            ftp: get_cell(row, map, "FTP"),
            fts: get_cell(row, map, "FTS"),
            haz: get_cell(row, map, "HAZ"),
            ins: get_cell(row, map, "INS"),
            loc: get_cell(row, map, "LOC"),
            ish: get_cell(row, map, "ISH"),
            lps: get_cell(row, map, "LPS"),
            slp: get_cell(row, map, "SLP"),
            lsc: get_cell(row, map, "LSC"),
            oms: get_cell(row, map, "OMS"),
            sov: get_cell(row, map, "SOV"),
            opu: get_cell(row, map, "OPU"),
            par: get_cell(row, map, "PAR"),
            pas: get_cell(row, map, "PAS"),
            pdl: get_cell(row, map, "PDL"),
            ppu: get_cell(row, map, "PPU"),
            psc: get_cell(row, map, "PSC"),
            rep: get_cell(row, map, "REP"),
            res: get_cell(row, map, "RES"),
            rtn: get_cell(row, map, "RTN"),
            sed: get_cell(row, map, "SED"),
            sur: get_cell(row, map, "SUR"),
            oth: get_cell(row, map, "OTH"),
            tax: get_cell(row, map, "TAX"),
            gst: get_cell(row, map, "GST"),
            hst: get_cell(row, map, "HST"),
            pst: get_cell(row, map, "PST"),
            qst: get_cell(row, map, "QST"),
            gst_pct: get_cell(row, map, "GST_PCT"),
            hst_pct: get_cell(row, map, "HST_PCT"),
            pst_pct: get_cell(row, map, "PST_PCT"),
            qst_pct: get_cell(row, map, "QST_PCT"),
            fsc_pct: get_cell(row, map, "FSC_PCT"),
            fsc_pub: get_cell(row, map, "FSC_Pub"),
            fsc_inc: get_cell(row, map, "FSC_Inc"),
            fsc_net: get_cell(row, map, "FSC_Net"),
            tot_acc: get_cell(row, map, "Tot_Acc"),
            rca: get_cell(row, map, "RCA"),
            scc: get_cell(row, map, "SCC"),
            tot_pub: get_cell(row, map, "Tot_Pub"),
            tot_inc: get_cell(row, map, "Tot_Inc"),
            tot_net: get_cell(row, map, "Tot_net"),
            tot_adj: get_cell(row, map, "Tot_Adj"),
            currency: get_cell(row, map, "Currency"),
            exchange_rate: get_cell(row, map, "Exchange_Rate"),
            exchange_from_to: get_cell(row, map, "Exchange_From>To"),
            rrdd: get_cell(row, map, "RRDD"),
            invoicenum: get_cell(row, map, "InvoiceNum"),
            pur: get_cell(row, map, "PUR"),
            bt: get_cell(row, map, "BT"),
            book: get_cell(row, map, "Book"),
            pg: get_cell(row, map, "PG"),
            shipmentid: get_cell(row, map, "ShipmentID"),
            ref1: get_cell(row, map, "Ref1"),
            ref2: get_cell(row, map, "Ref2"),
            ref3: get_cell(row, map, "Ref3"),
            userid: get_cell(row, map, "UserID"),
            msg_codes: get_cell(row, map, "Msg_Codes"),
            notes: get_cell(row, map, "Notes"),
            port: get_cell(row, map, "Port"),
            s_cntry: get_cell(row, map, "S_Cntry"),
            r_cntry: get_cell(row, map, "R_Cntry"),
            tp_cntry: get_cell(row, map, "TP_Cntry"),
            sender: get_cell(row, map, "Sender"),
            s_name: get_cell(row, map, "S_Name"),
            s_company: get_cell(row, map, "S_Company"),
            s_address: get_cell(row, map, "S_Address"),
            s_city: get_cell(row, map, "S_City"),
            s_state: get_cell(row, map, "S_State"),
            s_zip: get_cell(row, map, "S_Zip"),
            s_zip_corrected: get_cell(row, map, "S_Zip_Corrected"),
            receiver: get_cell(row, map, "Receiver"),
            r_name: get_cell(row, map, "R_Name"),
            r_company: get_cell(row, map, "R_Company"),
            r_address: get_cell(row, map, "R_Address"),
            r_city: get_cell(row, map, "R_City"),
            r_state: get_cell(row, map, "R_State"),
            r_zip: get_cell(row, map, "R_Zip"),
            thirdparty: get_cell(row, map, "ThirdParty"),
            tp_name: get_cell(row, map, "TP_Name"),
            tp_company: get_cell(row, map, "TP_Company"),
            tp_address: get_cell(row, map, "TP_Address"),
            tp_city: get_cell(row, map, "TP_City"),
            tp_state: get_cell(row, map, "TP_State"),
            tp_zip: get_cell(row, map, "TP_Zip"),
            orig_servicetext: get_cell(row, map, "Orig_ServiceText"),
            orig_zip: get_cell(row, map, "Orig_Zip"),
            orig_zone: get_cell(row, map, "Orig_Zone"),
            orig_wgt: get_cell(row, map, "Orig_Wgt"),
            orig_pub: get_cell(row, map, "Orig_Pub"),
            orig_inc: get_cell(row, map, "Orig_Inc"),
            orig_net: get_cell(row, map, "Orig_Net"),
            new_wgt: get_cell(row, map, "New_Wgt"),
            new_pub: get_cell(row, map, "New_Pub"),
            new_inc: get_cell(row, map, "New_Inc"),
            new_net: get_cell(row, map, "New_Net"),
            daily_rate: get_cell(row, map, "Daily_Rate"),
            tsc: get_cell(row, map, "TSC"),
            tsvc: get_cell(row, map, "TSVC"),
            shipper_name: get_cell(row, map, "Shipper_Name"),
            tfilter_ref: get_cell(row, map, "TFilter_Ref"),
            upload_invoice_file_name: String::new(),
            upload_invoice_file_date: String::new(),
        });
    }
    Ok(items)
}

/// 空运单号为空时用上一行的 track_num
fn restock_invoices(mut invoices: Vec<UpsInvoice>) -> Vec<UpsInvoice> {
    for i in 1..invoices.len() {
        if invoices[i].track_num.is_empty() {
            invoices[i].track_num = invoices[i - 1].track_num.clone();
        }
    }
    invoices
}
