use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::collections::HashMap;

// Configuration and Core Enums

// A map of cluster names to their Prometheus endpoint URLs
fn get_prometheus_url(cluster: &Cluster) -> &'static str {
    match cluster {
        Cluster::Popeye => "http://popeye-prometheus.flatironinstitute.org:80",
        Cluster::Rusty => "http://prometheus.flatironinstitute.org:80",
    }
}

// Using enums for type safety, similar to Python's Literal type
#[derive(Debug, Clone, Copy)]
pub enum Cluster {
    Popeye,
    Rusty,
}

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
    cluster: &Cluster,
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
    step: Option<&str>,
) -> Result<PrometheusResponse, Box<dyn std::error::Error>> {
    let base_url = get_prometheus_url(cluster);
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


fn test_query(
    query: &str,
    cluster: &Cluster,
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
    step: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let base_url = get_prometheus_url(cluster);
    let _client = Client::builder()
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

    println!("The url is {url} and the query is {params:?}");

    //let response = client.get(&url).query(&params).send()?;
    //response.error_for_status_ref()?; // Check for HTTP errors like 4xx or 5xx
    //
    //let body_text = response.text()?;
    //let result: PrometheusResponse = serde_json::from_str(&body_text)?;
    //
    //if result.status != "success" {
    //    return Err("Prometheus query was not successful".into());
    //}
    Ok(())
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


fn range_group_by(result: PrometheusResponse, metric: Grouping) -> HashMap<String, Vec<u64>> {
    let mut data_dict = HashMap::new();
    let metric_key = metric.to_string();

    for series in result.data.result {
        // First, try to get the group name from the metric object.
        // This is the normal path for queries that return multiple series.
        if let Some(group) = series.metric.get(&metric_key) {
            if let Some(values) = series.values {
                let parsed_values: Vec<u64> = values
                    .into_iter()
                    .filter_map(|(_, val_str)| val_str.parse().ok())
                    .collect();
                data_dict.insert(group.clone(), parsed_values);
            }
        }
        // NEW: If the metric object is empty, we've found our special case.
        else if series.metric.is_empty() {
            if let Some(values) = series.values {
                let parsed_values: Vec<u64> = values
                    .into_iter()
                    .filter_map(|(_, val_str)| val_str.parse().ok())
                    .collect();
                // Since there's no group name, we'll use a default key.
                // "Total" is a good, descriptive choice for this aggregate data.
                data_dict.insert("Total".to_string(), parsed_values);
            }
        }
        // If a series has metrics but not the one we're looking for, we ignore it.
    }
    data_dict
}


// --- Public API Functions ---

pub fn test_usage_by(
    cluster: Cluster,
    grouping: Grouping,
    resource: Resource,
    days: i64,
    step: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let start_time = now - Duration::days(days);

    let usage_query = usage_query(grouping, resource); // Assuming Cpus for now
    test_query(&usage_query, &cluster, start_time, Some(now), Some(step))
}
pub fn get_usage_by(
    cluster: Cluster,
    grouping: Grouping,
    resource: Resource,
    days: i64,
    step: &str,
) -> Result<HashMap<String, Vec<u64>>, Box<dyn std::error::Error>> {
    let now = Utc::now();
    let start_time = now - Duration::days(days);

    let usage_query = usage_query(grouping, resource); // Assuming Cpus for now
    let result = query(&usage_query, &cluster, start_time, Some(now), Some(step))?;

    Ok(range_group_by(result, grouping))
}

pub fn get_max_resource(
    cluster: Cluster,
    grouping: Option<Grouping>,
    resource: Resource,
    days: Option<i64>,
    step: Option<&str>,
) -> Result<HashMap<String, Vec<u64>>, Box<dyn std::error::Error>> {
    let now = Utc::now();
    let start_time = now - Duration::days(days.unwrap_or(0));
    
    let cap_query = capacity_query(grouping, resource); // Assuming Cpus
    let result = query(&cap_query, &cluster, start_time, Some(now), step)?;

    // if days is none, then instantaneous regular grou by
    // otherwise range groupby
    
    if let Some(g) = grouping {
        Ok(range_group_by(result, g))
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
