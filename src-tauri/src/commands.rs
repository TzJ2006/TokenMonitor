use crate::models::*;
use crate::parser::UsageParser;
use chrono::{Datelike, Local, NaiveDate};
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

pub struct AppState {
    pub parser: Arc<UsageParser>,
    pub refresh_interval: Arc<RwLock<u64>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            parser: Arc::new(UsageParser::new()),
            refresh_interval: Arc::new(RwLock::new(30)),
        }
    }
}

#[tauri::command]
pub async fn set_refresh_interval(interval: u64, state: State<'_, AppState>) -> Result<(), String> {
    let mut current = state.refresh_interval.write().await;
    *current = interval;
    Ok(())
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> Result<(), String> {
    state.parser.clear_cache();
    Ok(())
}

#[tauri::command]
pub async fn get_usage_data(
    provider: String,
    period: String,
    state: State<'_, AppState>,
) -> Result<UsagePayload, String> {
    let parser = &state.parser;

    match provider.as_str() {
        "claude" | "codex" => Ok(get_provider_data(parser, &provider, &period)?),
        "all" => {
            let claude = get_provider_data(parser, "claude", &period)?;
            let codex = get_provider_data(parser, "codex", &period)?;
            Ok(merge_payloads(claude, codex))
        }
        _ => Err(format!("Unknown provider: {}", provider)),
    }
}

fn get_provider_data(parser: &UsageParser, provider: &str, period: &str) -> Result<UsagePayload, String> {
    let now = Local::now();
    let today = now.format("%Y%m%d").to_string();

    Ok(match period {
        "5h" if provider == "codex" => parser.get_daily(provider, &today),
        "5h" => parser.get_blocks(provider, &today),
        "day" => parser.get_hourly(provider, &today),
        "week" => {
            let week_start = (now - chrono::Duration::days(now.weekday().num_days_from_monday() as i64))
                .format("%Y%m%d").to_string();
            parser.get_daily(provider, &week_start)
        }
        "month" => {
            let month_start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
                .unwrap().format("%Y%m%d").to_string();
            parser.get_daily(provider, &month_start)
        }
        "year" => {
            let year_start = NaiveDate::from_ymd_opt(now.year(), 1, 1)
                .unwrap().format("%Y%m%d").to_string();
            parser.get_monthly(provider, &year_start)
        }
        _ => return Err(format!("Unknown period: {}", period)),
    })
}

fn merge_payloads(mut c: UsagePayload, x: UsagePayload) -> UsagePayload {
    let mut bucket_map: std::collections::BTreeMap<String, ChartBucket> =
        std::collections::BTreeMap::new();
    for b in c.chart_buckets.iter().chain(x.chart_buckets.iter()) {
        let entry = bucket_map.entry(b.label.clone()).or_insert_with(|| ChartBucket {
            label: b.label.clone(),
            total: 0.0,
            segments: vec![],
        });
        entry.total += b.total;
        entry.segments.extend(b.segments.clone());
    }

    c.total_cost += x.total_cost;
    c.total_tokens += x.total_tokens;
    c.session_count += x.session_count;
    c.input_tokens += x.input_tokens;
    c.output_tokens += x.output_tokens;
    c.chart_buckets = bucket_map.into_values().collect();
    c.model_breakdown.extend(x.model_breakdown);
    c.five_hour_cost += x.five_hour_cost;
    c.from_cache = c.from_cache && x.from_cache;
    c
}
