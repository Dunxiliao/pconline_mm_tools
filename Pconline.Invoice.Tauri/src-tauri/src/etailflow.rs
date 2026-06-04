// Etailflow V2: POST /api/orders/shipping

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingOrderRequestWrapper {
    pub service_provider: i32,
    pub merchant_code: String,
    pub seller: String,
    pub shippings: Vec<ShipOrderRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipOrderRequest {
    pub order_number: String,
    pub carrier_name: String,
    pub service_code: String,
    pub packages: Vec<Package>,
    pub replace_tracking_number: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub tracking_number: String,
    pub lines: Vec<OrderLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLine {
    pub sku: String,
    pub quantity: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EtailflowResponseWrapper<T> {
    #[allow(dead_code)]
    pub code: Option<i32>,
    pub message: Option<String>,
    pub success: bool,
    pub data: Option<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShippingOrderResponse {
    pub order_number: String,
    pub status: String,
    pub message: Option<String>,
}

fn etailflow_base_url() -> String {
    crate::get_config("ETAILFLOW_URL").unwrap_or_else(|| "https://order.innovberg.com".to_string())
}

/// POST /api/orders/shipping. Returns per-order status from result.Data.
pub async fn post_shipping_order(
    access_token: &str,
    merchant_code: &str,
    service_provider: i32,
    seller: &str,
    shippings: Vec<ShipOrderRequest>,
) -> Result<EtailflowResponseWrapper<Vec<ShippingOrderResponse>>, String> {
    let base = etailflow_base_url();
    let url = format!("{}/api/orders/shipping", base.trim_end_matches('/'));
    let body = ShippingOrderRequestWrapper {
        service_provider,
        merchant_code: merchant_code.to_string(),
        seller: seller.to_string(),
        shippings,
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), text));
    }
    serde_json::from_str(&text).map_err(|e| format!("Parse response: {}", e))
}
