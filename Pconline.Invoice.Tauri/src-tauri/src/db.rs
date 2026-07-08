//! 数据库访问：发票校验与插入（PostgreSQL，schema: datas）
//! 连接串优先读取环境变量，其次回退到 build.rs 内嵌配置

use chrono::Utc;
use once_cell::sync::OnceCell;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

static POOL: OnceCell<PgPool> = OnceCell::new();
static WMS_POOL: OnceCell<PgPool> = OnceCell::new();
static UPS_TABLE_COLUMNS: OnceCell<std::collections::HashMap<String, String>> = OnceCell::new();

fn get_pool() -> Result<&'static PgPool, String> {
    POOL.get_or_try_init(|| {
        let url = crate::get_config("INVOICE_DB_URL")
            .ok_or_else(|| "INVOICE_DB_URL 未设置，请在环境变量或 .env 中配置".to_string())?;
        PgPoolOptions::new()
            .connect_lazy(&url)
            .map_err(|e| e.to_string())
    })
}

fn parse_invoice_numbers(invoice_numbers: &str) -> Vec<&str> {
    invoice_numbers
        .split('\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 查询 Amazon 元数据表中已存在的发票号（支持单个或换行分隔多个）
pub async fn check_amazon_invoice_exists(invoice_numbers: &str) -> Result<Vec<String>, String> {
    let pool = get_pool()?;
    let numbers = parse_invoice_numbers(invoice_numbers);
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT invoice_number FROM shipping_invoice_amazon_metadata WHERE invoice_number = ANY($1)",
    )
    .bind(&numbers)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// 查询 UPS 已存在的发票号（invoicenum）
pub async fn check_ups_invoice_exists(invoice_numbers: &str) -> Result<Vec<String>, String> {
    let pool = get_pool()?;
    let numbers = parse_invoice_numbers(invoice_numbers);
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT invoicenum FROM shipping_invoice_ups WHERE invoicenum = ANY($1)",
    )
    .bind(&numbers)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// 查询 Fedex 已存在的发票号（invoice_number）
pub async fn check_fedex_invoice_exists(invoice_numbers: &str) -> Result<Vec<String>, String> {
    let pool = get_pool()?;
    let numbers = parse_invoice_numbers(invoice_numbers);
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT invoice_number FROM shipping_invoice_fedex WHERE invoice_number = ANY($1)",
    )
    .bind(&numbers)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// 查询 OnTrac 已存在的发票号（invoice_number）
pub async fn check_ontrac_invoice_exists(invoice_numbers: &str) -> Result<Vec<String>, String> {
    let pool = get_pool()?;
    let numbers = parse_invoice_numbers(invoice_numbers);
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT invoice_number FROM shipping_invoice_ontrac WHERE invoice_number = ANY($1)",
    )
    .bind(&numbers)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// 插入 Amazon 发票：先插 metadata 再插 items（同一事务）
pub async fn insert_amazon_invoice(
    metadata: &crate::amazon::AmazonInvoiceMetadata,
    items: &[crate::amazon::AmazonInvoiceItem],
    adjustments: &[crate::amazon::AmazonInvoiceAdjustment],
) -> Result<(), String> {
    let pool = get_pool()?;
    let fallback_now = Utc::now().naive_utc();

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // 与 C# FreeSql 一致：表名与列名均为小写，upload_invoice_file_date 为 timestamp
    let id: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO shipping_invoice_amazon_metadata
        (invoice_number, account_name, account_number, service_period_start, service_period_end,
         invoice_date, due_date, upload_invoice_file_name, upload_invoice_file_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(&metadata.invoice_number)
    .bind(&metadata.account_name)
    .bind(&metadata.account_number)
    .bind(metadata.service_period_start.as_str())
    .bind(metadata.service_period_end.as_str())
    .bind(metadata.invoice_date.as_str())
    .bind(metadata.due_date.as_str())
    .bind(&metadata.upload_invoice_file_name)
    .bind(
        chrono::NaiveDateTime::parse_from_str(
            &metadata.upload_invoice_file_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap_or(fallback_now),
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| format!("metadata 插入失败: {}", e))?;

    for item in items {
        sqlx::query(
            r#"
            INSERT INTO shipping_invoice_amazon_shipping
            (invoice_id, tracking_id, order_id, reference, "from", from_zip, "to", to_zip, date_label_printed, shipping_service,
             date_of_charge, leg_type, comments, child_account_id, zone, base_rate_usd, discount_usd, delivery_area_surcharge_usd,
             fuel_surcharge_usd, additional_handeling_fee_usd, peak_demand_surcharge_usd, non_standard_fee_usd, pickup_charge_usd,
             tax_exclusive_total_charge_usd, tax_amount_usd, tax_inclusive_total_charge_usd,
             billable_weight, actual_weight, dimensional_divisor, length_in, width_in, height_in)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32)
            "#,
        )
        .bind(id)
        .bind(&item.tracking_id)
        .bind(&item.order_id)
        .bind(&item.reference)
        .bind(&item.from_place)
        .bind(&item.from_zip)
        .bind(&item.to_place)
        .bind(&item.to_zip)
        .bind(&item.date_label_printed)
        .bind(&item.shipping_service)
        .bind(&item.date_of_charge)
        .bind(&item.leg_type)
        .bind(&item.comments)
        .bind(&item.child_account_id)
        .bind(&item.zone)
        .bind(&item.base_rate_usd)
        .bind(&item.discount_usd)
        .bind(&item.delivery_area_surcharge_usd)
        .bind(&item.fuel_surcharge_usd)
        .bind(&item.additional_handeling_fee_usd)
        .bind(&item.peak_demand_surcharge_usd)
        .bind(&item.non_standard_fee_usd)
        .bind(&item.pickup_charge_usd)
        .bind(&item.tax_exclusive_total_charge_usd)
        .bind(&item.tax_amount_usd)
        .bind(&item.tax_inclusive_total_charge_usd)
        .bind(&item.billable_weight)
        .bind(&item.actual_weight)
        .bind(&item.dimensional_divisor)
        .bind(&item.length_in)
        .bind(&item.width_in)
        .bind(&item.height_in)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("items 插入失败: {}", e))?;
    }

    for item in adjustments {
        sqlx::query(
            r#"
            INSERT INTO shipping_invoice_amazon_shipping_adjustments
            (invoice_id, tracking_id, order_id, reference, date_label_printed, shipping_service,
             date_of_charge, charge_type, comments, base_rate_usd, tax_amount_usd, base_rate_incl_tax_usd)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(id)
        .bind(&item.tracking_id)
        .bind(&item.order_id)
        .bind(&item.reference)
        .bind(&item.date_label_printed)
        .bind(&item.shipping_service)
        .bind(&item.date_of_charge)
        .bind(&item.charge_type)
        .bind(&item.comments)
        .bind(&item.base_rate_usd)
        .bind(&item.tax_amount_usd)
        .bind(&item.base_rate_incl_tax_usd)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("adjustments 插入失败: {}", e))?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 插入 Temu 发票（无校验，直接插 shipping_invoice_temu）
pub async fn insert_temu_invoice(items: &[crate::temu::TemuInvoice]) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let pool = get_pool()?;
    let fallback_now = Utc::now().naive_utc();
    for item in items {
        let upload_invoice_file_date = chrono::NaiveDateTime::parse_from_str(
            &item.upload_invoice_file_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap_or(fallback_now);
        sqlx::query(
            r#"
            INSERT INTO shipping_invoice_temu
            (transaction_date, transaction_type, related_id, order_id, order_item_id, sku, sku_id, quantity,
             ship_city, ship_state, retail_price, platform_discount, seller_discount, service_fee, service_fee_tax,
             platform_incentive, subtotal, shipping, platform_incentive_shipping, product_tax, shipping_tax,
             marketplace_withheld_tax, others, total, currency, upload_invoice_file_name, upload_invoice_file_date)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27)
            "#,
        )
        .bind(&item.transaction_date)
        .bind(&item.transaction_type)
        .bind(&item.related_id)
        .bind(&item.order_id)
        .bind(&item.order_item_id)
        .bind(&item.sku)
        .bind(&item.sku_id)
        .bind(&item.quantity)
        .bind(&item.ship_city)
        .bind(&item.ship_state)
        .bind(&item.retail_price)
        .bind(&item.platform_discount)
        .bind(&item.seller_discount)
        .bind(&item.service_fee)
        .bind(&item.service_fee_tax)
        .bind(&item.platform_incentive)
        .bind(&item.subtotal)
        .bind(&item.shipping)
        .bind(&item.platform_incentive_shipping)
        .bind(&item.product_tax)
        .bind(&item.shipping_tax)
        .bind(&item.marketplace_withheld_tax)
        .bind(&item.others)
        .bind(&item.total)
        .bind(&item.currency)
        .bind(&item.upload_invoice_file_name)
        .bind(upload_invoice_file_date)
        .execute(pool)
        .await
        .map_err(|e| format!("Temu 插入失败: {}", e))?;
    }
    Ok(())
}

/// 插入 OnTrac 发票（无校验，直接插 shipping_invoice_ontrac）
pub async fn insert_ontrac_invoice(items: &[crate::ontrac::OnTracInvoice]) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let pool = get_pool()?;
    let fallback_now = Utc::now().naive_utc();
    for item in items {
        let upload_invoice_file_date = chrono::NaiveDateTime::parse_from_str(
            &item.upload_invoice_file_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap_or(fallback_now);
        sqlx::query(
            r#"
            INSERT INTO shipping_invoice_ontrac
            (invoice_number, billing_date, ontrac_destination_facility_code, reference1, reference2,
             customer_order_number, third_party_account_number, tracking_number, shipper_company_name,
             shipper_street, shipper_city, shipper_state, shipper_country, shipper_postalcode,
             destination_contact, destination_street, destination_city, destination_state,
             destination_country, destination_postalcode, service_code, zone, return_to_sender,
             residential, irregular_category, proof_of_delivery_name, proof_of_delivery_datetime,
             first_scan_datetime, customer_account, injection_postalcode, weight, length, width, height,
             billed_weight, billed_length, billed_width, billed_height, dim_factor, weight_source,
             total_charges, service_charges, address_correction_surcharge, delivery_intervention_required,
             extra_piece_surcharge, residential_surcharge, delivery_area_surcharge, extended_area_surcharge,
             additional_handling_surcharge, large_package_surcharge, over_maximum_limits_surcharge,
             signature_required, adult_signature_required, relabel_surcharge, weekend_surcharge,
             demand_surcharge, demand_additional_handling_surcharge, demand_large_package_surcharge,
             demand_over_maximum_limits_surcharge, on_call_pickup, shipping_charge_correction_audit_fee,
             volume_rebate, volume_rebate_2, volume_rebate_3, missing_pld, other_adjustments,
             miscellaneous_charges, fuel_surcharge, upload_invoice_file_name, upload_invoice_file_date)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19,
                    $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37,
                    $38, $39, $40, $41, $42, $43, $44, $45, $46, $47, $48, $49, $50, $51, $52, $53, $54, $55,
                    $56, $57, $58, $59, $60, $61, $62, $63, $64, $65, $66, $67, $68, $69, $70)
            "#,
        )
        .bind(&item.invoice_number)
        .bind(&item.billing_date)
        .bind(&item.ontrac_destination_facility_code)
        .bind(&item.reference1)
        .bind(&item.reference2)
        .bind(&item.customer_order_number)
        .bind(&item.third_party_account_number)
        .bind(&item.tracking_number)
        .bind(&item.shipper_company_name)
        .bind(&item.shipper_street)
        .bind(&item.shipper_city)
        .bind(&item.shipper_state)
        .bind(&item.shipper_country)
        .bind(&item.shipper_postalcode)
        .bind(&item.destination_contact)
        .bind(&item.destination_street)
        .bind(&item.destination_city)
        .bind(&item.destination_state)
        .bind(&item.destination_country)
        .bind(&item.destination_postalcode)
        .bind(&item.service_code)
        .bind(&item.zone)
        .bind(&item.return_to_sender)
        .bind(&item.residential)
        .bind(&item.irregular_category)
        .bind(&item.proof_of_delivery_name)
        .bind(&item.proof_of_delivery_datetime)
        .bind(&item.first_scan_datetime)
        .bind(&item.customer_account)
        .bind(&item.injection_postalcode)
        .bind(&item.weight)
        .bind(&item.length)
        .bind(&item.width)
        .bind(&item.height)
        .bind(&item.billed_weight)
        .bind(&item.billed_length)
        .bind(&item.billed_width)
        .bind(&item.billed_height)
        .bind(&item.dim_factor)
        .bind(&item.weight_source)
        .bind(&item.total_charges)
        .bind(&item.service_charges)
        .bind(&item.address_correction_surcharge)
        .bind(&item.delivery_intervention_required)
        .bind(&item.extra_piece_surcharge)
        .bind(&item.residential_surcharge)
        .bind(&item.delivery_area_surcharge)
        .bind(&item.extended_area_surcharge)
        .bind(&item.additional_handling_surcharge)
        .bind(&item.large_package_surcharge)
        .bind(&item.over_maximum_limits_surcharge)
        .bind(&item.signature_required)
        .bind(&item.adult_signature_required)
        .bind(&item.relabel_surcharge)
        .bind(&item.weekend_surcharge)
        .bind(&item.demand_surcharge)
        .bind(&item.demand_additional_handling_surcharge)
        .bind(&item.demand_large_package_surcharge)
        .bind(&item.demand_over_maximum_limits_surcharge)
        .bind(&item.on_call_pickup)
        .bind(&item.shipping_charge_correction_audit_fee)
        .bind(&item.volume_rebate)
        .bind(&item.volume_rebate_2)
        .bind(&item.volume_rebate_3)
        .bind(&item.missing_pld)
        .bind(&item.other_adjustments)
        .bind(&item.miscellaneous_charges)
        .bind(&item.fuel_surcharge)
        .bind(&item.upload_invoice_file_name)
        .bind(upload_invoice_file_date)
        .execute(pool)
        .await
        .map_err(|e| format!("OnTrac 插入失败: {}", e))?;
    }
    Ok(())
}

/// 插入 UPS 发票（尽量对齐 C# UpsInvoice：插入 Excel 表头对应的全部列）
pub async fn insert_ups_invoice(items: &[crate::ups::UpsInvoice]) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let pool = get_pool()?;
    let fallback_now = Utc::now().naive_utc();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    // 读取 UPS 表的列集合（避免插入 Excel 中不存在于表的列）
    // Map: lowercase_column_name -> actual_column_name
    let ups_columns: std::collections::HashMap<String, String> = if let Some(cached) = UPS_TABLE_COLUMNS.get() {
        cached.clone()
    } else {
        let cols = sqlx::query_scalar::<_, String>(
            r#"SELECT column_name
               FROM information_schema.columns
               WHERE table_schema = 'datas' AND table_name = 'shipping_invoice_ups'"#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        let map: std::collections::HashMap<String, String> = cols
            .into_iter()
            .map(|c| {
                let lc = c.to_lowercase();
                (lc, c)
            })
            .collect();
        let _ = UPS_TABLE_COLUMNS.set(map.clone());
        map
    };

    enum CellValue {
        Str(String),
        Num(f64),
        Time(chrono::NaiveDateTime),
    }

    fn parse_decimal_or_zero(raw: &str) -> f64 {
        let s = raw.trim();
        if s.is_empty() {
            return 0.0;
        }
        let normalized = s.replace(',', "");
        // 兼容会计格式：(123.45) => -123.45
        let normalized = if normalized.starts_with('(') && normalized.ends_with(')') {
            format!("-{}", &normalized[1..normalized.len() - 1])
        } else {
            normalized
        };
        normalized.parse::<f64>().unwrap_or(0.0)
    }

    for item in items {
        let upload_invoice_file_date = chrono::NaiveDateTime::parse_from_str(
            &item.upload_invoice_file_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap_or(fallback_now);

        // 用有序 map 组装列和值，保证占位符顺序稳定
        let mut values: std::collections::BTreeMap<String, CellValue> = std::collections::BTreeMap::new();

        fn insert_value(
            values: &mut std::collections::BTreeMap<String, CellValue>,
            ups_columns: &std::collections::HashMap<String, String>,
            col: &str,
            val: CellValue,
        ) -> Result<(), String> {
            match ups_columns.get(col) {
                Some(actual_col) => {
                    values.insert(actual_col.clone(), val);
                    Ok(())
                }
                None => Err(format!("UPS 表缺少列: {}", col)),
            }
        }
        fn insert_str_value(
            values: &mut std::collections::BTreeMap<String, CellValue>,
            ups_columns: &std::collections::HashMap<String, String>,
            col: &str,
            v: &str,
        ) -> Result<(), String> {
            insert_value(values, ups_columns, col, CellValue::Str(v.to_string()))
        }
        insert_value(&mut values, &ups_columns, "track_num", CellValue::Str(item.track_num.clone()))?;
        insert_value(&mut values, &ups_columns, "track_num_alt", CellValue::Str(item.track_num_alt.clone()))?;
        insert_value(&mut values, &ups_columns, "rpt", CellValue::Str(item.rpt.clone()))?;
        insert_value(&mut values, &ups_columns, "source_area", CellValue::Str(item.source_area.clone()))?;
        insert_value(&mut values, &ups_columns, "service_text", CellValue::Str(item.service_text.clone()))?;
        insert_value(&mut values, &ups_columns, "act_num", CellValue::Str(item.act_num.clone()))?;
        insert_value(&mut values, &ups_columns, "act_num_shipped", CellValue::Str(item.act_num_shipped.clone()))?;
        insert_value(&mut values, &ups_columns, "act_num_billed", CellValue::Str(item.act_num_billed.clone()))?;
        insert_value(&mut values, &ups_columns, "invdate", CellValue::Str(item.invdate.clone()))?;
        insert_value(&mut values, &ups_columns, "pudate", CellValue::Str(item.pudate.clone()))?;
        insert_value(&mut values, &ups_columns, "inout", CellValue::Str(item.inout.clone()))?;
        insert_value(&mut values, &ups_columns, "mvmt", CellValue::Str(item.mvmt.clone()))?;
        insert_value(&mut values, &ups_columns, "chgtyp", CellValue::Str(item.chgtyp.clone()))?;
        insert_value(&mut values, &ups_columns, "pdt", CellValue::Str(item.pdt.clone()))?;
        insert_value(&mut values, &ups_columns, "ctn", CellValue::Str(item.ctn.clone()))?;
        insert_value(&mut values, &ups_columns, "svc", CellValue::Str(item.svc.clone()))?;
        insert_value(&mut values, &ups_columns, "rc", CellValue::Str(item.rc.clone()))?;
        insert_value(&mut values, &ups_columns, "spmt", CellValue::Str(item.spmt.clone()))?;
        insert_value(&mut values, &ups_columns, "spmt_num", CellValue::Str(item.spmt_num.clone()))?;
        insert_value(&mut values, &ups_columns, "billtyp", CellValue::Str(item.billtyp.clone()))?;
        insert_value(&mut values, &ups_columns, "pkgs", CellValue::Str(item.pkgs.clone()))?;
        insert_value(&mut values, &ups_columns, "cnt", CellValue::Str(item.cnt.clone()))?;
        insert_value(&mut values, &ups_columns, "zip", CellValue::Str(item.zip.clone()))?;
        insert_value(&mut values, &ups_columns, "zone", CellValue::Str(item.zone.clone()))?;
        insert_value(&mut values, &ups_columns, "act_wgt", CellValue::Str(item.act_wgt.clone()))?;
        insert_value(&mut values, &ups_columns, "wgt_audited", CellValue::Str(item.wgt_audited.clone()))?;
        insert_value(&mut values, &ups_columns, "bill_wgt", CellValue::Str(item.bill_wgt.clone()))?;
        insert_value(&mut values, &ups_columns, "wgt_uom", CellValue::Str(item.wgt_uom.clone()))?;
        insert_value(&mut values, &ups_columns, "wgt_src", CellValue::Str(item.wgt_src.clone()))?;
        insert_value(&mut values, &ups_columns, "os", CellValue::Str(item.os.clone()))?;
        insert_value(&mut values, &ups_columns, "l", CellValue::Str(item.l.clone()))?;
        insert_value(&mut values, &ups_columns, "w", CellValue::Str(item.w.clone()))?;
        insert_value(&mut values, &ups_columns, "h", CellValue::Str(item.h.clone()))?;
        insert_value(&mut values, &ups_columns, "cubic", CellValue::Str(item.cubic.clone()))?;
        insert_value(&mut values, &ups_columns, "l_ke", CellValue::Str(item.l_ke.clone()))?;
        insert_value(&mut values, &ups_columns, "w_ke", CellValue::Str(item.w_ke.clone()))?;
        insert_value(&mut values, &ups_columns, "h_ke", CellValue::Str(item.h_ke.clone()))?;
        insert_value(&mut values, &ups_columns, "dim_uom", CellValue::Str(item.dim_uom.clone()))?;
        insert_value(&mut values, &ups_columns, "pub", CellValue::Str(item.r#pub.clone()))?;
        insert_value(&mut values, &ups_columns, "inc", CellValue::Str(item.inc.clone()))?;
        insert_value(&mut values, &ups_columns, "net", CellValue::Str(item.net.clone()))?;
        insert_str_value(&mut values, &ups_columns, "ahc_t", &item.ahc_t)?;
        insert_str_value(&mut values, &ups_columns, "bop_t", &item.bop_t)?;
        insert_str_value(&mut values, &ups_columns, "das_t", &item.das_t)?;
        insert_str_value(&mut values, &ups_columns, "dc_t", &item.dc_t)?;
        insert_str_value(&mut values, &ups_columns, "di_t", &item.di_t)?;
        insert_str_value(&mut values, &ups_columns, "haz_t", &item.haz_t)?;
        insert_str_value(&mut values, &ups_columns, "ins_t", &item.ins_t)?;
        insert_str_value(&mut values, &ups_columns, "ins_units", &item.ins_units)?;
        insert_str_value(&mut values, &ups_columns, "declared_value", &item.declared_value)?;
        insert_str_value(&mut values, &ups_columns, "lps_t", &item.lps_t)?;
        insert_str_value(&mut values, &ups_columns, "oms_t", &item.oms_t)?;
        insert_str_value(&mut values, &ups_columns, "opu_t", &item.opu_t)?;
        insert_str_value(&mut values, &ups_columns, "pas_t", &item.pas_t)?;
        insert_str_value(&mut values, &ups_columns, "pdl_t", &item.pdl_t)?;
        insert_str_value(&mut values, &ups_columns, "ppu_t", &item.ppu_t)?;
        insert_str_value(&mut values, &ups_columns, "psc_t", &item.psc_t)?;
        insert_str_value(&mut values, &ups_columns, "rtn_t", &item.rtn_t)?;
        insert_str_value(&mut values, &ups_columns, "adc", &item.adc)?;
        insert_str_value(&mut values, &ups_columns, "ahc", &item.ahc)?;
        insert_str_value(&mut values, &ups_columns, "sah", &item.sah)?;
        insert_str_value(&mut values, &ups_columns, "bop", &item.bop)?;
        insert_str_value(&mut values, &ups_columns, "cbf", &item.cbf)?;
        insert_str_value(&mut values, &ups_columns, "cns", &item.cns)?;
        insert_str_value(&mut values, &ups_columns, "cnv", &item.cnv)?;
        insert_str_value(&mut values, &ups_columns, "cod", &item.cod)?;
        insert_str_value(&mut values, &ups_columns, "coo", &item.coo)?;
        insert_str_value(&mut values, &ups_columns, "das", &item.das)?;
        insert_str_value(&mut values, &ups_columns, "dc", &item.dc)?;
        insert_str_value(&mut values, &ups_columns, "ddo", &item.ddo)?;
        insert_str_value(&mut values, &ups_columns, "di", &item.di)?;
        insert_str_value(&mut values, &ups_columns, "dtr", &item.dtr)?;
        insert_str_value(&mut values, &ups_columns, "faf", &item.faf)?;
        insert_str_value(&mut values, &ups_columns, "ftp", &item.ftp)?;
        insert_str_value(&mut values, &ups_columns, "fts", &item.fts)?;
        insert_str_value(&mut values, &ups_columns, "haz", &item.haz)?;
        insert_str_value(&mut values, &ups_columns, "ins", &item.ins)?;
        insert_str_value(&mut values, &ups_columns, "loc", &item.loc)?;
        insert_str_value(&mut values, &ups_columns, "ish", &item.ish)?;
        insert_str_value(&mut values, &ups_columns, "lps", &item.lps)?;
        insert_str_value(&mut values, &ups_columns, "slp", &item.slp)?;
        insert_str_value(&mut values, &ups_columns, "lsc", &item.lsc)?;
        insert_str_value(&mut values, &ups_columns, "oms", &item.oms)?;
        insert_str_value(&mut values, &ups_columns, "sov", &item.sov)?;
        insert_str_value(&mut values, &ups_columns, "opu", &item.opu)?;
        insert_str_value(&mut values, &ups_columns, "par", &item.par)?;
        insert_str_value(&mut values, &ups_columns, "pas", &item.pas)?;
        insert_str_value(&mut values, &ups_columns, "pdl", &item.pdl)?;
        insert_str_value(&mut values, &ups_columns, "ppu", &item.ppu)?;
        insert_str_value(&mut values, &ups_columns, "psc", &item.psc)?;
        insert_str_value(&mut values, &ups_columns, "rep", &item.rep)?;
        insert_str_value(&mut values, &ups_columns, "res", &item.res)?;
        insert_str_value(&mut values, &ups_columns, "rtn", &item.rtn)?;
        insert_str_value(&mut values, &ups_columns, "sed", &item.sed)?;
        insert_str_value(&mut values, &ups_columns, "sur", &item.sur)?;
        insert_str_value(&mut values, &ups_columns, "oth", &item.oth)?;
        insert_str_value(&mut values, &ups_columns, "tax", &item.tax)?;
        insert_str_value(&mut values, &ups_columns, "gst", &item.gst)?;
        insert_str_value(&mut values, &ups_columns, "hst", &item.hst)?;
        insert_str_value(&mut values, &ups_columns, "pst", &item.pst)?;
        insert_str_value(&mut values, &ups_columns, "qst", &item.qst)?;
        insert_str_value(&mut values, &ups_columns, "gst_pct", &item.gst_pct)?;
        insert_str_value(&mut values, &ups_columns, "hst_pct", &item.hst_pct)?;
        insert_str_value(&mut values, &ups_columns, "pst_pct", &item.pst_pct)?;
        insert_str_value(&mut values, &ups_columns, "qst_pct", &item.qst_pct)?;
        insert_str_value(&mut values, &ups_columns, "fsc_pct", &item.fsc_pct)?;
        insert_str_value(&mut values, &ups_columns, "fsc_pub", &item.fsc_pub)?;
        insert_str_value(&mut values, &ups_columns, "fsc_inc", &item.fsc_inc)?;
        insert_str_value(&mut values, &ups_columns, "fsc_net", &item.fsc_net)?;
        insert_str_value(&mut values, &ups_columns, "tot_acc", &item.tot_acc)?;
        insert_str_value(&mut values, &ups_columns, "rca", &item.rca)?;
        insert_str_value(&mut values, &ups_columns, "scc", &item.scc)?;
        insert_str_value(&mut values, &ups_columns, "tot_pub", &item.tot_pub)?;
        insert_str_value(&mut values, &ups_columns, "tot_inc", &item.tot_inc)?;
        insert_value(&mut values, &ups_columns, "tot_net", CellValue::Num(parse_decimal_or_zero(&item.tot_net)))?;
        insert_value(&mut values, &ups_columns, "tot_adj", CellValue::Str(item.tot_adj.clone()))?;
        insert_str_value(&mut values, &ups_columns, "currency", &item.currency)?;
        insert_str_value(&mut values, &ups_columns, "exchange_rate", &item.exchange_rate)?;
        insert_str_value(&mut values, &ups_columns, "exchange_from>to", &item.exchange_from_to)?;
        insert_str_value(&mut values, &ups_columns, "rrdd", &item.rrdd)?;
        insert_str_value(&mut values, &ups_columns, "invoicenum", &item.invoicenum)?;
        insert_str_value(&mut values, &ups_columns, "pur", &item.pur)?;
        insert_str_value(&mut values, &ups_columns, "bt", &item.bt)?;
        insert_str_value(&mut values, &ups_columns, "book", &item.book)?;
        insert_str_value(&mut values, &ups_columns, "pg", &item.pg)?;
        insert_str_value(&mut values, &ups_columns, "shipmentid", &item.shipmentid)?;
        insert_str_value(&mut values, &ups_columns, "ref1", &item.ref1)?;
        insert_str_value(&mut values, &ups_columns, "ref2", &item.ref2)?;
        insert_str_value(&mut values, &ups_columns, "ref3", &item.ref3)?;
        insert_str_value(&mut values, &ups_columns, "userid", &item.userid)?;
        insert_str_value(&mut values, &ups_columns, "msg_codes", &item.msg_codes)?;
        insert_str_value(&mut values, &ups_columns, "notes", &item.notes)?;
        insert_str_value(&mut values, &ups_columns, "port", &item.port)?;
        insert_str_value(&mut values, &ups_columns, "s_cntry", &item.s_cntry)?;
        insert_str_value(&mut values, &ups_columns, "r_cntry", &item.r_cntry)?;
        insert_str_value(&mut values, &ups_columns, "tp_cntry", &item.tp_cntry)?;
        insert_str_value(&mut values, &ups_columns, "sender", &item.sender)?;
        insert_str_value(&mut values, &ups_columns, "s_name", &item.s_name)?;
        insert_str_value(&mut values, &ups_columns, "s_company", &item.s_company)?;
        insert_str_value(&mut values, &ups_columns, "s_address", &item.s_address)?;
        insert_str_value(&mut values, &ups_columns, "s_city", &item.s_city)?;
        insert_str_value(&mut values, &ups_columns, "s_state", &item.s_state)?;
        insert_str_value(&mut values, &ups_columns, "s_zip", &item.s_zip)?;
        insert_str_value(&mut values, &ups_columns, "s_zip_corrected", &item.s_zip_corrected)?;
        insert_str_value(&mut values, &ups_columns, "receiver", &item.receiver)?;
        insert_str_value(&mut values, &ups_columns, "r_name", &item.r_name)?;
        insert_str_value(&mut values, &ups_columns, "r_company", &item.r_company)?;
        insert_str_value(&mut values, &ups_columns, "r_address", &item.r_address)?;
        insert_str_value(&mut values, &ups_columns, "r_city", &item.r_city)?;
        insert_str_value(&mut values, &ups_columns, "r_state", &item.r_state)?;
        insert_str_value(&mut values, &ups_columns, "r_zip", &item.r_zip)?;
        insert_str_value(&mut values, &ups_columns, "thirdparty", &item.thirdparty)?;
        insert_str_value(&mut values, &ups_columns, "tp_name", &item.tp_name)?;
        insert_str_value(&mut values, &ups_columns, "tp_company", &item.tp_company)?;
        insert_str_value(&mut values, &ups_columns, "tp_address", &item.tp_address)?;
        insert_str_value(&mut values, &ups_columns, "tp_city", &item.tp_city)?;
        insert_str_value(&mut values, &ups_columns, "tp_state", &item.tp_state)?;
        insert_str_value(&mut values, &ups_columns, "tp_zip", &item.tp_zip)?;
        insert_str_value(&mut values, &ups_columns, "orig_servicetext", &item.orig_servicetext)?;
        insert_str_value(&mut values, &ups_columns, "orig_zip", &item.orig_zip)?;
        insert_str_value(&mut values, &ups_columns, "orig_zone", &item.orig_zone)?;
        insert_str_value(&mut values, &ups_columns, "orig_wgt", &item.orig_wgt)?;
        insert_str_value(&mut values, &ups_columns, "orig_pub", &item.orig_pub)?;
        insert_str_value(&mut values, &ups_columns, "orig_inc", &item.orig_inc)?;
        insert_str_value(&mut values, &ups_columns, "orig_net", &item.orig_net)?;
        insert_str_value(&mut values, &ups_columns, "new_wgt", &item.new_wgt)?;
        insert_str_value(&mut values, &ups_columns, "new_pub", &item.new_pub)?;
        insert_str_value(&mut values, &ups_columns, "new_inc", &item.new_inc)?;
        insert_str_value(&mut values, &ups_columns, "new_net", &item.new_net)?;
        insert_str_value(&mut values, &ups_columns, "daily_rate", &item.daily_rate)?;
        insert_str_value(&mut values, &ups_columns, "tsc", &item.tsc)?;
        insert_str_value(&mut values, &ups_columns, "tsvc", &item.tsvc)?;
        insert_str_value(&mut values, &ups_columns, "shipper_name", &item.shipper_name)?;
        insert_str_value(&mut values, &ups_columns, "tfilter_ref", &item.tfilter_ref)?;
        insert_str_value(&mut values, &ups_columns, "upload_invoice_file_name", &item.upload_invoice_file_name)?;

        // upload_invoice_file_date：timestamp(6)
        match ups_columns.get("upload_invoice_file_date") {
            Some(actual_col) => {
                values.insert(actual_col.clone(), CellValue::Time(upload_invoice_file_date));
            }
            None => return Err("UPS 表缺少列: upload_invoice_file_date".to_string()),
        }

        let columns: Vec<String> = values.keys().cloned().collect();
        let columns_sql = columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(",");

        let placeholders = (1..=columns.len())
            .map(|i| format!("${}", i))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "INSERT INTO shipping_invoice_ups ({}) VALUES ({})",
            columns_sql, placeholders
        );

        let mut q = sqlx::query(&sql);
        for col in &columns {
            match values.get(col).expect("value exists") {
                CellValue::Str(s) => {
                    q = q.bind(s);
                }
                CellValue::Num(n) => {
                    q = q.bind(n);
                }
                CellValue::Time(t) => {
                    q = q.bind(t);
                }
            }
        }

        q.execute(&mut *tx)
            .await
            .map_err(|e| format!("UPS 插入失败: {}", e))?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 插入 Fedex 发票（invoices + charges）
pub async fn insert_fedex_invoice(info: &crate::fedex::FedexInfo) -> Result<(), String> {
    fn parse_decimal_or_zero(raw: &str) -> f64 {
        let s = raw.trim();
        if s.is_empty() {
            return 0.0;
        }
        let normalized = s.replace(',', "");
        let normalized = if normalized.starts_with('(') && normalized.ends_with(')') {
            format!("-{}", &normalized[1..normalized.len() - 1])
        } else {
            normalized
        };
        normalized.parse::<f64>().unwrap_or(0.0)
    }

    let pool = get_pool()?;
    let fallback_now = Utc::now().naive_utc();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    for inv in &info.invoices {
        let upload_invoice_file_date = chrono::NaiveDateTime::parse_from_str(
            &inv.upload_invoice_file_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap_or(fallback_now);

        sqlx::query(
            r#"
            INSERT INTO shipping_invoice_fedex
            (consolidated_account, bill_to_account_number, invoice_date, invoice_number, store_id, original_amount_due,
             current_balance, payor, ground_tracking_id_prefix, express_or_ground_tracking_id, transportation_charge_amount,
             net_charge_amount, service_type, ground_service, shipment_date, pod_delivery_date, pod_delivery_time,
             pod_service_area_code, pod_signature_description, actual_weight_amount, actual_weight_units, rated_weight_amount,
             rated_weight_units, number_of_pieces, bundle_number, meter_number, tdmastertrackingid, service_packaging,
             dim_length, dim_width, dim_height, dim_divisor, dim_unit, recipient_name, recipient_company, recipient_address_line_1,
             recipient_address_line_2, recipient_city, recipient_state, recipient_zip_code, recipient_country_or_territory,
             shipper_company, shipper_name, shipper_address_line_1, shipper_address_line_2, shipper_city, shipper_state,
             shipper_zip_code, shipper_country_or_territory, original_customer_reference, original_ref_2, original_ref_3_or_po_number,
             original_department_reference_description, updated_customer_reference, updated_ref_2, updated_ref_3_or_po_number,
             updated_department_reference_description, rma_, original_recipient_address_line_1, original_recipient_address_line_2,
             original_recipient_city, original_recipient_state, original_recipient_zip_code, original_recipient_country_or_territory,
             zone_code, cost_allocation, alternate_address_line_1, alternate_address_line_2, alternate_city,
             alternate_state_province, alternate_zip_code, alternate_country_or_territory_code, crossreftrackingid_prefix,
             crossreftrackingid, entry_date, entry_number, customs_value, customs_value_currency_code, declared_value,
             declared_value_currency_code, commodity_description, commodity_country_or_territory_code, commodity_description_1,
             commodity_country_or_territory_code_1, commodity_description_2, commodity_country_or_territory_code_2,
             commodity_description_3, commodity_country_or_territory_code_3, currency_conversion_date, currency_conversion_rate,
             multiweight_number, multiweight_total_multiweight_units, multiweight_total_multiweight_weight,
             multiweight_total_shipment_charge_amount, multiweight_total_shipment_weight,
             ground_tracking_id_address_correction_discount_charge_amount,
             ground_tracking_id_address_correction_gross_charge_amount, rated_method, sort_hub, estimated_weight,
             estimated_weight_unit, postal_class, process_category, package_size, delivery_confirmation, tendered_date,
             mps_package_id, shipment_notes, upload_invoice_file_name, upload_invoice_file_date)
            VALUES ($1, $2, $3, $4, $5, $6,
                    $7, $8, $9, $10, $11, $12,
                    $13, $14, $15, $16, $17, $18,
                    $19, $20, $21, $22, $23, $24,
                    $25, $26, $27, $28, $29, $30,
                    $31, $32, $33, $34, $35, $36,
                    $37, $38, $39, $40, $41, $42,
                    $43, $44, $45, $46, $47, $48,
                    $49, $50, $51, $52, $53, $54,
                    $55, $56, $57, $58, $59, $60,
                    $61, $62, $63, $64, $65, $66,
                    $67, $68, $69, $70, $71, $72,
                    $73, $74, $75, $76, $77, $78,
                    $79, $80, $81, $82, $83, $84,
                    $85, $86, $87, $88, $89, $90,
                    $91, $92, $93, $94, $95, $96,
                    $97, $98, $99, $100, $101, $102,
                    $103, $104, $105, $106, $107, $108,
                    $109, $110)
            "#,
        )
        .bind(&inv.consolidated_account)
        .bind(&inv.bill_to_account_number)
        .bind(&inv.invoice_date)
        .bind(&inv.invoice_number)
        .bind(&inv.store_id)
        .bind(&inv.original_amount_due)
        .bind(&inv.current_balance)
        .bind(&inv.payor)
        .bind(&inv.ground_tracking_id_prefix)
        .bind(&inv.express_or_ground_tracking_id)
        .bind(&inv.transportation_charge_amount)
        .bind(parse_decimal_or_zero(&inv.net_charge_amount))
        .bind(&inv.service_type)
        .bind(&inv.ground_service)
        .bind(&inv.shipment_date)
        .bind(&inv.pod_delivery_date)
        .bind(&inv.pod_delivery_time)
        .bind(&inv.pod_service_area_code)
        .bind(&inv.pod_signature_description)
        .bind(&inv.actual_weight_amount)
        .bind(&inv.actual_weight_units)
        .bind(&inv.rated_weight_amount)
        .bind(&inv.rated_weight_units)
        .bind(&inv.number_of_pieces)
        .bind(&inv.bundle_number)
        .bind(&inv.meter_number)
        .bind(&inv.tdmastertrackingid)
        .bind(&inv.service_packaging)
        .bind(&inv.dim_length)
        .bind(&inv.dim_width)
        .bind(&inv.dim_height)
        .bind(&inv.dim_divisor)
        .bind(&inv.dim_unit)
        .bind(&inv.recipient_name)
        .bind(&inv.recipient_company)
        .bind(&inv.recipient_address_line_1)
        .bind(&inv.recipient_address_line_2)
        .bind(&inv.recipient_city)
        .bind(&inv.recipient_state)
        .bind(&inv.recipient_zip_code)
        .bind(&inv.recipient_country_or_territory)
        .bind(&inv.shipper_company)
        .bind(&inv.shipper_name)
        .bind(&inv.shipper_address_line_1)
        .bind(&inv.shipper_address_line_2)
        .bind(&inv.shipper_city)
        .bind(&inv.shipper_state)
        .bind(&inv.shipper_zip_code)
        .bind(&inv.shipper_country_or_territory)
        .bind(&inv.original_customer_reference)
        .bind(&inv.original_ref_2)
        .bind(&inv.original_ref_3_or_po_number)
        .bind(&inv.original_department_reference_description)
        .bind(&inv.updated_customer_reference)
        .bind(&inv.updated_ref_2)
        .bind(&inv.updated_ref_3_or_po_number)
        .bind(&inv.updated_department_reference_description)
        .bind(&inv.rma_)
        .bind(&inv.original_recipient_address_line_1)
        .bind(&inv.original_recipient_address_line_2)
        .bind(&inv.original_recipient_city)
        .bind(&inv.original_recipient_state)
        .bind(&inv.original_recipient_zip_code)
        .bind(&inv.original_recipient_country_or_territory)
        .bind(&inv.zone_code)
        .bind(&inv.cost_allocation)
        .bind(&inv.alternate_address_line_1)
        .bind(&inv.alternate_address_line_2)
        .bind(&inv.alternate_city)
        .bind(&inv.alternate_state_province)
        .bind(&inv.alternate_zip_code)
        .bind(&inv.alternate_country_or_territory_code)
        .bind(&inv.crossreftrackingid_prefix)
        .bind(&inv.crossreftrackingid)
        .bind(&inv.entry_date)
        .bind(&inv.entry_number)
        .bind(&inv.customs_value)
        .bind(&inv.customs_value_currency_code)
        .bind(&inv.declared_value)
        .bind(&inv.declared_value_currency_code)
        .bind(&inv.commodity_description)
        .bind(&inv.commodity_country_or_territory_code)
        .bind(&inv.commodity_description_1)
        .bind(&inv.commodity_country_or_territory_code_1)
        .bind(&inv.commodity_description_2)
        .bind(&inv.commodity_country_or_territory_code_2)
        .bind(&inv.commodity_description_3)
        .bind(&inv.commodity_country_or_territory_code_3)
        .bind(&inv.currency_conversion_date)
        .bind(&inv.currency_conversion_rate)
        .bind(&inv.multiweight_number)
        .bind(&inv.multiweight_total_multiweight_units)
        .bind(&inv.multiweight_total_multiweight_weight)
        .bind(&inv.multiweight_total_shipment_charge_amount)
        .bind(&inv.multiweight_total_shipment_weight)
        .bind(&inv.ground_tracking_id_address_correction_discount_charge_amount)
        .bind(&inv.ground_tracking_id_address_correction_gross_charge_amount)
        .bind(&inv.rated_method)
        .bind(&inv.sort_hub)
        .bind(&inv.estimated_weight)
        .bind(&inv.estimated_weight_unit)
        .bind(&inv.postal_class)
        .bind(&inv.process_category)
        .bind(&inv.package_size)
        .bind(&inv.delivery_confirmation)
        .bind(&inv.tendered_date)
        .bind(&inv.mps_package_id)
        .bind(&inv.shipment_notes)
        .bind(&inv.upload_invoice_file_name)
        .bind(upload_invoice_file_date)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Fedex 插入失败: {}", e))?;
    }

    for ch in &info.charges {
        let amount = parse_decimal_or_zero(&ch.charge_amount);
        let upload_invoice_file_date = chrono::NaiveDateTime::parse_from_str(
            &ch.upload_invoice_file_date,
            "%Y-%m-%d %H:%M:%S",
        )
        .unwrap_or(fallback_now);
        sqlx::query(
            r#"
            INSERT INTO shipping_invoice_fedex_charge
            (express_or_ground_tracking_id, charge_type, charge_amount, upload_invoice_file_name, upload_invoice_file_date)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&ch.express_or_ground_tracking_id)
        .bind(&ch.charge_type)
        .bind(amount)
        .bind(&ch.upload_invoice_file_name)
        .bind(upload_invoice_file_date)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Fedex charge 插入失败: {}", e))?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- WMS 数据库（Payment：etailflow_client_accounts, sale_order, sale_order_package）----------
fn get_wms_pool() -> Result<&'static PgPool, String> {
    WMS_POOL.get_or_try_init(|| {
        let url = crate::get_config("WMS_DB_URL")
            .ok_or_else(|| "WMS_DB_URL 未设置，请在环境变量或 .env 中配置".to_string())?;
        PgPoolOptions::new()
            .connect_lazy(&url)
            .map_err(|e| e.to_string())
    })
}

async fn get_client_account_id(customer_code: &str) -> Result<Option<i32>, String> {
    let pool = get_wms_pool()?;
    let code = customer_code.trim();
    if code.is_empty() {
        return Ok(None);
    }

    // 保持原有语义：仍然对列做 LOWER(TRIM(code)) 匹配
    let row = sqlx::query_scalar::<_, i32>(
        r#"SELECT id
           FROM etailflow_client_accounts
           WHERE LOWER(TRIM(code)) = LOWER(TRIM($1))
           LIMIT 1"#,
    )
    .bind(code)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
}

/// 批量查询：返回在库中存在的客户 code 集合（LOWER(TRIM(Code))）
pub async fn payment_get_existing_customers(customer_names: &[String]) -> Result<Vec<String>, String> {
    let pool = get_wms_pool()?;
    let normalized: Vec<String> = customer_names
        .iter()
        .filter_map(|n| {
            let t = n.trim().to_lowercase();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if normalized.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT LOWER(TRIM(Code)) FROM etailflow_client_accounts WHERE LOWER(TRIM(Code)) = ANY($1)",
    )
    .bind(&normalized)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().filter(|r| !r.is_empty()).collect())
}

/// Operation Fee：更新 client_operation_fee_paid = true，返回 (总条数, 更新条数)
pub async fn payment_update_operation_fee_paid(
    order_ids: &[String],
    customer_code: &str,
) -> Result<(usize, u64), String> {
    let pool = get_wms_pool()?;
    let list: Vec<String> = order_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if list.is_empty() || customer_code.trim().is_empty() {
        return Ok((list.len(), 0));
    }
    let total = list.len();

    let client_account_id = get_client_account_id(customer_code).await?;
    if client_account_id.is_none() {
        return Ok((total, 0));
    }
    let client_account_id = client_account_id.unwrap();

    let affected = sqlx::query(
        r#"UPDATE sale_order SET "client_operation_fee_paid" = true
WHERE client_order_ref = ANY($1) AND client_account_id = $2"#,
    )
    .bind(&list)
    .bind(client_account_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    Ok((total, affected))
}

/// 查询已标记为已付的 Order ID 集合
pub async fn payment_get_matched_operation_order_ids(
    order_ids: &[String],
    customer_code: &str,
) -> Result<std::collections::HashSet<String>, String> {
    let pool = get_wms_pool()?;
    let list: Vec<String> = order_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if list.is_empty() || customer_code.trim().is_empty() {
        return Ok(std::collections::HashSet::new());
    }

    let client_account_id = get_client_account_id(customer_code).await?;
    let Some(client_account_id) = client_account_id else {
        return Ok(std::collections::HashSet::new());
    };

    let rows = sqlx::query_scalar::<_, String>(
        r#"SELECT client_order_ref
           FROM sale_order
           WHERE client_order_ref = ANY($1)
             AND client_account_id = $2
             AND "client_operation_fee_paid" = true"#,
    )
    .bind(&list)
    .bind(client_account_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let set: std::collections::HashSet<String> = rows
        .into_iter()
        .filter_map(|r| {
            let t = r.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        })
        .collect();
    Ok(set)
}

/// Shipping Fee 两轮匹配。返回 (total, round1_by_tracking, round2_by_order_count, unmatched, matched_trackings, round2_order_names, multi_package_order_names)
pub async fn payment_update_shipping_fee_paid(
    rows: &[(String, String)], // (order_number, tracking_number)
    customer_code: &str,
) -> Result<
    (
        usize,
        usize,
        usize,
        usize,
        std::collections::HashSet<String>,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    let pool = get_wms_pool()?;
    let code = customer_code.trim();
    if code.is_empty() {
        return Ok((
            rows.len(),
            0,
            0,
            rows.len(),
            std::collections::HashSet::new(),
            Vec::new(),
            Vec::new(),
        ));
    }

    let client_account_id = get_client_account_id(customer_code).await?;
    let Some(client_account_id) = client_account_id else {
        // 与原实现一致：子查询找不到 id 时后续查询都返回空，最终 unmatched = total
        return Ok((
            rows.len(),
            0,
            0,
            rows.len(),
            std::collections::HashSet::new(),
            Vec::new(),
            Vec::new(),
        ));
    };

    let trackings: Vec<String> = rows
        .iter()
        .filter_map(|(_, t)| {
            let t = t.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut matched_by_tracking = std::collections::HashSet::new();
    if !trackings.is_empty() {
        let exist: Vec<String> = sqlx::query_scalar(
            r#"SELECT p.tracking_id FROM sale_order_package p
INNER JOIN sale_order so ON so.id = p.order_id AND so.client_account_id = $1
WHERE p.tracking_id = ANY($2)"#,
        )
        .bind(client_account_id)
        .bind(&trackings)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        for m in exist {
            let t = m.trim();
            if !t.is_empty() {
                matched_by_tracking.insert(t.to_string());
            }
        }
        sqlx::query(
            r#"UPDATE sale_order_package p SET "client_shipping_fee_paid" = true
FROM sale_order so WHERE so.id = p.order_id AND so.client_account_id = $1 AND p.tracking_id = ANY($2)"#,
        )
        .bind(client_account_id)
        .bind(&trackings)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    let round1 = rows
        .iter()
        .filter(|(_, t)| !t.trim().is_empty() && matched_by_tracking.contains(t.trim()))
        .count();

    let unmatched_order_numbers: Vec<String> = rows
        .iter()
        .filter(|(_, t)| t.trim().is_empty() || !matched_by_tracking.contains(t.trim()))
        .filter_map(|(o, _)| {
            let o = o.trim();
            if o.is_empty() {
                None
            } else {
                Some(o.to_string())
            }
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut round2_matched = Vec::new();
    let mut multi_package = Vec::new();
    if !unmatched_order_numbers.is_empty() {
        let single: Vec<String> = sqlx::query_scalar(
            r#"SELECT TRIM(so.client_order_ref) FROM sale_order so
INNER JOIN sale_order_package p ON so.id = p.order_id AND so.client_account_id = $1
WHERE TRIM(so.client_order_ref) = ANY($2) GROUP BY TRIM(so.client_order_ref) HAVING COUNT(*) = 1"#,
        )
        .bind(client_account_id)
        .bind(&unmatched_order_numbers)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        round2_matched = single.into_iter().filter(|s| !s.is_empty()).collect();

        let multi: Vec<String> = sqlx::query_scalar(
            r#"SELECT TRIM(so.client_order_ref) FROM sale_order so
INNER JOIN sale_order_package p ON so.id = p.order_id AND so.client_account_id = $1
WHERE TRIM(so.client_order_ref) = ANY($2) GROUP BY TRIM(so.client_order_ref) HAVING COUNT(*) > 1"#,
        )
        .bind(client_account_id)
        .bind(&unmatched_order_numbers)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        multi_package = multi.into_iter().filter(|s| !s.is_empty()).collect();

        sqlx::query(
            r#"UPDATE sale_order_package p SET "client_shipping_fee_paid" = true
FROM (SELECT p2.order_id FROM sale_order_package p2
INNER JOIN sale_order so ON so.id = p2.order_id AND so.client_account_id = $1
WHERE TRIM(so.client_order_ref) = ANY($2) GROUP BY p2.order_id HAVING COUNT(*) = 1) AS single
WHERE p.order_id = single.order_id"#,
        )
        .bind(client_account_id)
        .bind(&unmatched_order_numbers)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    let round2_count = round2_matched.len();
    let total = rows.len();
    let unmatched = total.saturating_sub(round1).saturating_sub(round2_count);
    Ok((
        total,
        round1,
        round2_count,
        unmatched,
        matched_by_tracking,
        round2_matched,
        multi_package,
    ))
}
