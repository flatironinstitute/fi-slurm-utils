use chrono::{DateTime, Datelike, Days, Duration, Utc};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::collections::HashMap;

// Configuration and Core Enums

#[derive(Debug, Clone, Copy)]
pub enum Grouping {
    Account,
    Nodes,
    GpuType,
}

// Helper to convert the Grouping enum to its string representation for queries
impl std::fmt::Display for Grouping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Grouping::Account => write!(f, "account"),
            Grouping::Nodes => write!(f, "nodes"),
            Grouping::GpuType => write!(f, "gputype"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Resource {
    Cpus,
    Bytes,
    Gpus,
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Resource::Cpus => write!(f, "cpus"),
            Resource::Bytes => write!(f, "bytes"),
            Resource::Gpus => write!(f, "gpus"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrometheusTimeScale {
    Minutes,
    Hours,
    #[default]
    Days,
    Weeks,
    Years,
}

impl std::fmt::Display for PrometheusTimeScale {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PrometheusTimeScale::Minutes => write!(f, "1m"),
            PrometheusTimeScale::Hours => write!(f, "1h"),
            PrometheusTimeScale::Days => write!(f, "1d"), 
            PrometheusTimeScale::Weeks => write!(f, "1w"),
            PrometheusTimeScale::Years => write!(f, "1y"),
        }
    }
}

impl PrometheusTimeScale {
    pub fn next(&self) -> Self {
        match self {
            Self::Minutes => Self::Hours,
            Self::Hours => Self::Days,
            Self::Days => Self::Weeks,
            Self::Weeks => Self::Years,
            Self::Years=> Self::Minutes, // Wraps around
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Minutes => Self::Years,
            Self::Hours => Self::Minutes,
            Self::Days => Self::Hours,
            Self::Weeks => Self::Days,
            Self::Years => Self::Weeks,
        }
    }
}

struct TimeRangeReturn {
    now: DateTime<Utc>,
    start_time: DateTime<Utc>,
}

fn get_time_range(
    increments: i64,
    step: &PrometheusTimeScale,
) -> TimeRangeReturn {

    let now = Utc::now();

    let start_time = match step {
        PrometheusTimeScale::Minutes => {now - Duration::minutes(increments)},
        PrometheusTimeScale::Hours => {now - Duration::hours(increments)},
        PrometheusTimeScale::Days => now.checked_sub_days(Days::new(increments as u64)).unwrap(),
        PrometheusTimeScale::Weeks => now.checked_sub_days(Days::new(increments as u64 * 7)).unwrap(),
        // PrometheusTimeScale::Months => now.checked_sub_months(Months::new(increments as u32)).unwrap(),
        PrometheusTimeScale::Years => {
            let current_year = now.year();
            now.with_year(current_year - increments as i32).unwrap()
        }
    };

    TimeRangeReturn {now, start_time}
}

// Structs for Deserializing Prometheus JSON Response

#[derive(Deserialize, Debug)]
struct PrometheusResponse {
    status: String,
    data: PrometheusData,
}

#[derive(Deserialize, Debug)]
struct PrometheusData {
    #[serde(rename = "resultType")]
    _result_type: String,
    result: Vec<PrometheusResult>,
}

#[derive(Deserialize, Debug)]
struct PrometheusResult {
    metric: HashMap<String, String>,
    // For instant queries, `value` will be present
    value: Option<(f64, String)>,
    // For range queries, `values` will be present
    values: Option<Vec<(f64, String)>>,
}

fn usage_query(grouping: Grouping, resource: Resource) -> String {
    format!(
        "sum by({grouping}) (slurm_job_{resource}{{state=\"running\",job=\"slurm\"}})")
}

fn capacity_query(grouping: Option<Grouping>, resource: Resource) -> String {
    let by_clause =
        grouping.map_or_else(String::new, |g| format!("by({g})"));
    format!(
        "sum {by_clause} (slurm_node_{resource}{{state!=\"drain\",state!=\"down\"}})")
}


/// The core function for querying the Prometheus API
fn query(
    query: &str,
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
    step: Option<PrometheusTimeScale>,
) -> Result<PrometheusResponse, Box<dyn std::error::Error>> {
    let base_url = "http://prometheus/";
    let client = Client::builder()
        .danger_accept_invalid_certs(true) // Equivalent to `verify=False`
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let mut params = HashMap::new();
    params.insert("query".to_string(), query.to_string());
    params.insert("start".to_string(), start.timestamp().to_string());

    let url = if let (Some(end_time), Some(step_val)) = (end, step) {
        params.insert("end".to_string(), end_time.timestamp().to_string());
        params.insert("step".to_string(), step_val.to_string());
        format!("{base_url}/api/v1/query_range")
    } else {
        format!("{base_url}/api/v1/query")
    };

    let response = client.get(&url).query(&params).send()?;
    response.error_for_status_ref()?; // Check for HTTP errors like 4xx or 5xx

    let body_text = response.text()?;
    let result: PrometheusResponse = serde_json::from_str(&body_text)?;

    if result.status != "success" {
        return Err("Prometheus query was not successful".into());
    }

    Ok(result)
}

/// Processes an instant query result.
#[allow(dead_code)]
fn group_by(result: PrometheusResponse, metric: Grouping) -> HashMap<String, u64> {
    let mut data_dict = HashMap::new();
    let metric_key = metric.to_string();

    for series in result.data.result {
        if let Some(group) = series.metric.get(&metric_key) {
            if let Some((_, value_str)) = series.value {
                if let Ok(value) = value_str.parse::<u64>() {
                    data_dict.insert(group.clone(), value);
                }
            }
        }
    }
    data_dict
}

/// Fills missing data points with zero for a range query result
fn range_group_by(
    result: PrometheusResponse,
    metric: Grouping,
    start_time: DateTime<Utc>,
    step: PrometheusTimeScale,
    increments: i64,
) -> HashMap<String, Vec<u64>> {
    // Determine step size in seconds
    let step_secs: i64 = match step {
        PrometheusTimeScale::Minutes => 60,
        PrometheusTimeScale::Hours => 3600,
        PrometheusTimeScale::Days => 86400,
        PrometheusTimeScale::Weeks => 86400 * 7,
        PrometheusTimeScale::Years => 86400 * 365,
    };
    let metric_key = metric.to_string();
    // Collect raw timestamp->value maps per group
    let mut raw: HashMap<String, HashMap<i64, u64>> = HashMap::new();
    for series in result.data.result {
        // Determine group key: metric value or default "Total"
        let group_key = if let Some(g) = series.metric.get(&metric_key) {
            g.clone()
        } else if series.metric.is_empty() {
            "Total".to_string()
        } else {
            continue;
        };
        if let Some(values) = series.values {
            let entry = raw.entry(group_key).or_default();
            for (ts_f, val_str) in values {
                let ts = ts_f as i64;
                if let Ok(v) = val_str.parse::<u64>() {
                    entry.insert(ts, v);
                }
            }
        }
    }
    // Build filled series for each group
    let mut filled: HashMap<String, Vec<u64>> = HashMap::new();
    for (group, map) in raw.into_iter() {
        let mut series = Vec::with_capacity((increments + 1) as usize);
        let mut t = start_time.timestamp();
        for _ in 0..=increments {
            let v = map.get(&t).copied().unwrap_or(0);
            series.push(v);
            t += step_secs;
        }
        filled.insert(group, series);
    }
    filled
}


// --- Public API Functions ---

pub fn get_usage_by(
    grouping: Grouping,
    resource: Resource,
    increments: i64,
    step: PrometheusTimeScale,
) -> Result<HashMap<String, Vec<u64>>, Box<dyn std::error::Error>> {
    let time_return = get_time_range(increments, &step);
    let now = time_return.now;
    let start_time = time_return.start_time;

    let usage_query = usage_query(grouping, resource); // Assuming Cpus for now
    let result = query(&usage_query, start_time, Some(now), Some(step))?;

    // Fill missing data points with zeros
    Ok(range_group_by(result, grouping, start_time, step, increments))
}

pub fn get_max_resource(
    grouping: Option<Grouping>,
    resource: Resource,
    increments: i64,
    step: PrometheusTimeScale,
) -> Result<HashMap<String, Vec<u64>>, Box<dyn std::error::Error>> {
    let time_return = get_time_range(increments, &step);
    let now = time_return.now;
    let start_time = time_return.start_time;
    
    let cap_query = capacity_query(grouping, resource); // Assuming Cpus
    let result = query(&cap_query, start_time, Some(now), Some(step))?;

    // if days is none, then instantaneous regular groupby
    // otherwise range groupby
    
    if let Some(g) = grouping {
        // For grouped capacity, fill missing data points
        Ok(range_group_by(result, g, start_time, step, increments))
    } else {
        // Handle case where there is no grouping
        let mut total = 0;
        if let Some(series) = result.data.result.first() {
            if let Some((_, val_str)) = &series.value {
                total = val_str.parse().unwrap_or(0);
            }
        }
        let mut map = HashMap::new();
        map.insert("total".to_string(), vec![total]);
        Ok(map)
    }
}
