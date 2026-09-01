//! Pull Database & Assign Order：Odoo + EfShip 查询，与 C# OdooOrderRepository / EfShipRepository 一致

use once_cell::sync::OnceCell;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::tracking::{TrackingOrderDto, TrackingPackageDto};

static ODOO_POOL: OnceCell<PgPool> = OnceCell::new();
static EFSHIP_POOL: OnceCell<PgPool> = OnceCell::new();

fn get_odoo_pool() -> Result<&'static PgPool, String> {
    ODOO_POOL.get_or_try_init(|| {
        let url =
            crate::get_config("ODOO_DB_URL").ok_or_else(|| "ODOO_DB_URL 未设置".to_string())?;
        PgPoolOptions::new()
            .connect_lazy(&url)
            .map_err(|e| e.to_string())
    })
}

fn get_efship_pool() -> Result<&'static PgPool, String> {
    EFSHIP_POOL.get_or_try_init(|| {
        let url =
            crate::get_config("EFSHIP_DB_URL").ok_or_else(|| "EFSHIP_DB_URL 未设置".to_string())?;
        PgPoolOptions::new()
            .connect_lazy(&url)
            .map_err(|e| e.to_string())
    })
}

/// Odoo: 根据供应商获取待回传订单号（provider 4=Walmart, 3/8=Newegg）
pub async fn odoo_get_order_numbers(provider: i32) -> Result<Vec<String>, String> {
    let odoo_provider = match provider {
        4 => 4,
        3 | 8 => 3,
        _ => return Err("Goflow does not support pulling from the database.".to_string()),
    };
    let sql = match odoo_provider {
        4 => "SELECT name FROM sale_order WHERE walmart_status in ('created', 'acknowledged') AND buy_label_from='EF_SHIP' AND state != 'cancel'",
        3 => "SELECT name FROM sale_order WHERE newegg_status='Unshipped' AND buy_label_from = 'EF_SHIP' AND STATE != 'cancel'",
        _ => return Err("Unsupported provider for Pull Database.".to_string()),
    };
    let pool = get_odoo_pool()?;
    let rows = sqlx::query_scalar::<_, String>(sql)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Odoo: 根据 Shipping Label Log 名称获取订单号
pub async fn odoo_get_order_numbers_by_label_name(label_name: &str) -> Result<Vec<String>, String> {
    let pool = get_odoo_pool()?;
    let sql = r#"
        SELECT so."name" FROM amazon_custom_order_lable_log log
        INNER JOIN amazon_custom_order_lable_log_sale_order_rel lo ON lo.amazon_custom_order_lable_log_id = log."id"
        INNER JOIN sale_order so ON so."id" = lo.sale_order_id
        WHERE log."name" = $1 AND so.buy_label_from = 'EF_SHIP' AND so.STATE != 'cancel'
    "#;
    let rows = sqlx::query_scalar::<_, String>(sql)
        .bind(label_name)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[derive(Debug, sqlx::FromRow)]
struct GiftOrderRow {
    name: String,
    original_order_name: Option<String>,
}

/// Odoo: 获取礼品订单映射（仅 provider=64 Goflow）
pub async fn odoo_get_gift_orders(order_names: &[String], provider: i32) -> Result<Vec<(String, String)>, String> {
    if provider != 64 || order_names.is_empty() {
        return Ok(Vec::new());
    }
    let pool = get_odoo_pool()?;
    let sql = r#"
        WITH sales AS (
            SELECT "id", "name" FROM sale_order WHERE "name" = ANY($1)
        )
        SELECT so."name", sl.original_order_id AS original_order_name
        FROM sale_order_line sl
        INNER JOIN sales so ON so."id" = sl.order_id
        WHERE sl.original_order_id IS NOT NULL AND sl.original_order_id <> '' AND sl.original_order_id != so."name"
    "#;
    let rows = sqlx::query_as::<_, GiftOrderRow>(sql)
        .bind(order_names)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.original_order_name.map(|o| (r.name.clone(), o)))
        .collect())
}

#[derive(Debug, sqlx::FromRow)]
struct OrderTrackingRow {
    order_number: String,
    tracking_number: Option<String>,
    service_code: Option<String>,
    carrier_name: Option<String>,
    sku: Option<String>,
    qty: Option<i32>,
}

/// EfShip: 根据订单号列表拉取出运信息（存储过程 get_shipment_details）
pub async fn efship_get_orders(order_numbers: &[String], provider: i32) -> Result<Vec<TrackingOrderDto>, String> {
    if order_numbers.is_empty() {
        return Ok(Vec::new());
    }
    let pool = get_efship_pool()?;
    let sql = "SELECT * FROM get_shipment_details($1)";
    let rows = sqlx::query_as::<_, OrderTrackingRow>(sql)
        .bind(order_numbers)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    use std::collections::HashMap;
    let mut by_order: HashMap<String, Vec<OrderTrackingRow>> = HashMap::new();
    for row in rows {
        by_order
            .entry(row.order_number.clone())
            .or_default()
            .push(row);
    }
    let mut result = Vec::with_capacity(by_order.len());
    for (order_number, rows) in by_order {
        let first = rows.first().unwrap();
        let tracking_number: Vec<String> = rows
            .iter()
            .filter_map(|r| r.tracking_number.as_ref())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let tracking_number = tracking_number.join(",");
        let mut service_code = first.service_code.clone().unwrap_or_default();
        if provider == 7 {
            service_code = service_code
                .replace('®', "")
                .replace(' ', "_")
                .to_lowercase();
        }
        let packages: Vec<TrackingPackageDto> = rows
            .iter()
            .map(|x| TrackingPackageDto {
                sku: x.sku.clone().unwrap_or_default(),
                qty: x.qty.unwrap_or(0),
                tracking_number: x.tracking_number.clone().unwrap_or_default(),
            })
            .collect();
        result.push(TrackingOrderDto {
            order_number,
            tracking_number,
            carrier_name: first.carrier_name.clone().unwrap_or_default(),
            service_code,
            status: String::new(),
            packages,
        });
    }
    Ok(result)
}
