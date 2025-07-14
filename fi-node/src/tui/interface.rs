use crate::tui::app::{AppError, CapacityData, FetchedData, UsageData};
use fi_prometheus::{get_max_resource, get_usage_by, Cluster, Grouping, Resource};
use tokio::sync::mpsc;

// --- Prometheus Interface ---


pub enum PrometheusTimeScale {
    Minute,
    Hour,
    Day,
    Week,
    Year,
}

struct PrometheusRequest {
    cluster: Cluster, //assume it's the one we're currently connected to? Try to get popeye info
    //from here?
    grouping: Option<Grouping>,
    resource: Resource,
    range: i64,
    time_scale: PrometheusTimeScale,
}

impl PrometheusRequest {
    fn new(
        cluster: Cluster, //assume it's the one we're currently connected to? Try to get popeye info
        //from here?
        grouping: Option<Grouping>,
        resource: Resource,
        range: i64,
        time_scale: PrometheusTimeScale,
    ) -> Self {
        Self {
            cluster,
            grouping,
            resource,
            range,
            time_scale,
        }
    }
}

// used to select which type of data to fetch
pub enum PrometheusDataType {
    Usage,
    Capacity,
}

// This enum is the successful return type. It can hold either
// a UsageData struct or a CapacityData struct
#[derive(Debug)]
pub enum PrometheusDataResult {
    Usage(UsageData),
    Capacity(CapacityData),
}

#[inline(always)]
fn prometheus_data_request(
    request: PrometheusRequest,
    data_type: PrometheusDataType,
) -> Result<PrometheusDataResult, AppError> {
    let time_scale = match request.time_scale {
        PrometheusTimeScale::Minute => "1m".to_string(),
        PrometheusTimeScale::Hour => "1h".to_string(),
        PrometheusTimeScale::Day => "1d".to_string(),
        PrometheusTimeScale::Week => "1w".to_string(),
        PrometheusTimeScale::Year => "1Y".to_string(),
    };

    match data_type {
        PrometheusDataType::Usage => {
            let data = get_usage_by(
                request.cluster,
                request.grouping.unwrap(), // No longer needs .unwrap()
                request.resource,
                request.range,
                &time_scale,
            )
            .map_err(|e| AppError::DataFetch(e.to_string()))?;

            Ok(PrometheusDataResult::Usage(UsageData {
                source_data: data,
            }))
        },

        PrometheusDataType::Capacity => {
            let data = get_max_resource(
                request.cluster,
                request.grouping, // get_max_resource expects an Option
                request.resource,
                Some(request.range), // This function also expects an Option
                Some(&time_scale),
            )
            .map_err(|e| AppError::DataFetch(e.to_string()))?;

            Ok(PrometheusDataResult::Capacity(CapacityData {
                capacities: data,
            }))
        },
    }
}

// --- CPU by Account ---


pub fn get_cpu_by_account_data(range: i64, time_scale: PrometheusTimeScale) -> Result<UsageData, AppError> {

    let request = PrometheusRequest::new( 
        Cluster::Rusty, 
        Some(Grouping::Account), 
        Resource::Cpus, 
        range, 
        time_scale,
    );

    let result = prometheus_data_request(request, PrometheusDataType::Usage)?;

    match result {
        PrometheusDataResult::Usage(usage_data) => Ok(usage_data),
        PrometheusDataResult::Capacity(_) => {
            Err(AppError::DataFetch("Unexpected data type returned. Expected Usage.".to_string()))
        }
    }
}

pub async fn get_cpu_by_account_data_async(tx: mpsc::Sender<FetchedData>, range: i64, time_scale: PrometheusTimeScale) {
    let result = tokio::task::spawn_blocking(move || get_cpu_by_account_data(range, time_scale)).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::CpuByAccount(data_res),
        Err(e) => FetchedData::CpuByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

pub fn get_cpu_capacity_by_account(range: i64, time_scale: PrometheusTimeScale) -> Result<CapacityData, AppError> {

    let request = PrometheusRequest::new( 
        Cluster::Rusty, 
        Some(Grouping::Account), 
        Resource::Cpus, 
        range, 
        time_scale
    );

    let result = prometheus_data_request(request, PrometheusDataType::Capacity)?;

    match result {
        PrometheusDataResult::Capacity(capacity_data) => Ok(capacity_data),
        PrometheusDataResult::Usage(_) => {
            Err(AppError::DataFetch("Unexpected data type returned. Expected Capacity.".to_string()))
        }
    }
}

pub async fn get_cpu_capacity_by_account_async(tx: mpsc::Sender<FetchedData>, range: i64, time_scale: PrometheusTimeScale) {
    let result = tokio::task::spawn_blocking(move || get_cpu_capacity_by_account(range, time_scale)).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByAccount(data),
        Err(e) => FetchedData::CpuCapacityByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

// --- CPU by Node ---

pub fn get_cpu_by_node_data(range: i64, time_scale: PrometheusTimeScale) -> Result<UsageData, AppError> {

    let request = PrometheusRequest::new( 
        Cluster::Rusty, 
        Some(Grouping::Nodes), 
        Resource::Cpus, 
        range, 
        time_scale,
    );

    let result = prometheus_data_request(request, PrometheusDataType::Usage)?;

    match result {
        PrometheusDataResult::Usage(usage_data) => Ok(usage_data),
        PrometheusDataResult::Capacity(_) => {
            Err(AppError::DataFetch("Unexpected data type returned. Expected Usage.".to_string()))
        }
    }
}

pub async fn get_cpu_by_node_data_async(tx: mpsc::Sender<FetchedData>, range: i64, time_scale: PrometheusTimeScale) {
    let result = tokio::task::spawn_blocking(move || get_cpu_by_node_data(range, time_scale)).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::CpuByNode(data_res),
        Err(e) => FetchedData::CpuByNode(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

pub fn get_cpu_capacity_by_node(range: i64, time_scale: PrometheusTimeScale) -> Result<CapacityData, AppError> {

    let request = PrometheusRequest::new( 
        Cluster::Rusty, 
        Some(Grouping::Nodes), 
        Resource::Cpus, 
        range, 
        time_scale,
    );

    let result = prometheus_data_request(request, PrometheusDataType::Capacity)?;

    match result {
        PrometheusDataResult::Capacity(capacity_data) => Ok(capacity_data),
        PrometheusDataResult::Usage(_) => {
            Err(AppError::DataFetch("Unexpected data type returned. Expected Capacity.".to_string()))
        }
    }
}

pub async fn get_cpu_capacity_by_node_async(tx: mpsc::Sender<FetchedData>, range: i64, time_scale: PrometheusTimeScale) {
    let result = tokio::task::spawn_blocking(move || get_cpu_capacity_by_node(range, time_scale)).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByNode(data),
        Err(e) => FetchedData::CpuCapacityByNode(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

// --- GPU by Type ---

pub fn get_gpu_by_type_data(range: i64, time_scale: PrometheusTimeScale) -> Result<UsageData, AppError> {

    let request = PrometheusRequest::new( 
        Cluster::Rusty, 
        Some(Grouping::GpuType), 
        Resource::Gpus, 
        range, 
        time_scale,
    );

    let result = prometheus_data_request(request, PrometheusDataType::Usage)?;

    match result {
        PrometheusDataResult::Usage(usage_data) => Ok(usage_data),
        PrometheusDataResult::Capacity(_) => {
            Err(AppError::DataFetch("Unexpected data type returned. Expected Usage.".to_string()))
        }
    }
}

pub async fn get_gpu_by_type_data_async(tx: mpsc::Sender<FetchedData>, range: i64, time_scale: PrometheusTimeScale) {
    let result = tokio::task::spawn_blocking(move || get_gpu_by_type_data(range, time_scale)).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::GpuByType(data_res),
        Err(e) => FetchedData::GpuByType(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

pub fn get_gpu_capacity_by_type(range: i64, time_scale: PrometheusTimeScale) -> Result<CapacityData, AppError> {

    let request = PrometheusRequest::new( 
        Cluster::Rusty, 
        Some(Grouping::GpuType), 
        Resource::Gpus, 
        range, 
        time_scale,
    );

    let result = prometheus_data_request(request, PrometheusDataType::Capacity)?;

    match result {
        PrometheusDataResult::Capacity(capacity_data) => Ok(capacity_data),
        PrometheusDataResult::Usage(_) => {
            Err(AppError::DataFetch("Unexpected data type returned. Expected Capacity.".to_string()))
        }
    }
}

pub async fn get_gpu_capacity_by_type_async(tx: mpsc::Sender<FetchedData>, range: i64, time_scale: PrometheusTimeScale) {
    let result = tokio::task::spawn_blocking(move || get_gpu_capacity_by_type(range, time_scale)).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::GpuCapacityByType(data),
        Err(e) => FetchedData::GpuCapacityByType(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

