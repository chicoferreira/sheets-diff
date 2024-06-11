use std::collections::HashMap;
use std::error::Error;
use std::iter::Iterator;
use std::time::Duration;

use anyhow::Context;
use google_sheets4 as sheets4;
use google_sheets4::hyper::Client;
use google_sheets4::hyper::client::HttpConnector;
use google_sheets4::hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use log::{debug, info, warn};
use serde_json::Value;
use sheets4::oauth2::{self, InstalledFlowAuthenticator, InstalledFlowReturnMethod};
use sheets4::Sheets;
use tokio;

type SheetsClient = Sheets<HttpsConnector<HttpConnector>>;
type SheetsContent = Vec<Vec<Value>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let hub = authenticate("client_secret.json").await?;
    let spreadsheet_id = std::env::var("SPREADSHEET_ID").context("SPREADSHEET_ID not found in env")?;
    let range = std::env::var("RANGE").context("RANGE not found in env")?;
    let webhook_url = std::env::var("WEBHOOK_URL").context("WEBHOOK_URL not found in env")?;

    let ids = load_ids("ids.txt");
    info!("Loaded ids: {:?}", ids);

    let mut current_data = get_sheet_values(&hub, &spreadsheet_id, &range).await?;
    info!("Starting loop");
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        match tick(&hub, &spreadsheet_id, &range, &webhook_url, &ids, &current_data).await {
            Ok(new_data) => current_data = new_data,
            Err(e) => warn!("{:?}", e),
        }
    }
}

async fn tick(hub: &SheetsClient,
              spreadsheet_id: &str,
              range: &str,
              webhook_url: &str,
              ids: &HashMap<String, String>,
              previous_data: &SheetsContent) -> anyhow::Result<SheetsContent> {
    let new_data: SheetsContent = get_sheet_values_timeout(&hub, &spreadsheet_id, &range).await?;
    debug!("New data: {}", serde_json::to_string(&new_data)?);
    for (new_row, old_row) in new_data.iter().zip(previous_data.iter()) {
        if new_row != old_row {
            info!("New row difference found at {:?}", serde_json::to_string(new_row));
            let content = new_row
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<String>>()
                .join(", ");

            let numero_aluno = new_row.get(0).context("No first row")?.as_str().context("First row not a string")?.to_uppercase();
            let extra = ids.get(numero_aluno.as_str()).map(|id| format!("<@{id}> ")).unwrap_or_default();

            let content = format!("{}{}", extra, content);

            reqwest::Client::new()
                .post(webhook_url)
                .json(&serde_json::json!({"content": content}))
                .send().await?;
        }
    }
    Ok(new_data)
}

fn load_ids(ids_path: &str) -> HashMap<String, String> {
    std::fs::read_to_string(ids_path).and_then(
        |content| Ok(content.lines().filter_map(|line| {
            let mut parts = line.split_whitespace();
            let key = parts.next()?.to_string();
            let value = parts.next()?.to_string().to_string();
            Some((key, value))
        }).collect())
    ).unwrap_or_default()
}

async fn get_sheet_values(sheets: &SheetsClient, spreadsheet_id: &str, range: &str) -> anyhow::Result<SheetsContent> {
    let response = sheets.spreadsheets().values_get(spreadsheet_id, range).doit().await?;
    let values = response.1.values.ok_or("No data found").map_err(anyhow::Error::msg)?;
    Ok(values)
}

async fn get_sheet_values_timeout(sheets: &SheetsClient, spreadsheet_id: &str, range: &str) -> anyhow::Result<SheetsContent> {
    tokio::time::timeout(
        Duration::from_secs(5),
        get_sheet_values(sheets, spreadsheet_id, range),
    ).await.context("The request timed out.")?
}

async fn authenticate(client_secret_file_path: &str) -> Result<SheetsClient, Box<dyn Error>> {
    let secret = oauth2::read_application_secret(client_secret_file_path).await?;
    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk("token.json")
        .build()
        .await?;

    let connector = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();
    let hub = Sheets::new(Client::builder().build(connector), auth);
    Ok(hub)
}