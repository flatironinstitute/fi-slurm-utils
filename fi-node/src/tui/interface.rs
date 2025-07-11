use crate::tui::app::{AppError, CapacityData, FetchedData, UsageData};
use fi_prometheus::{get_max_resource, get_usage_by, Cluster, Grouping, Resource};
use tokio::sync::mpsc;

// --- Prometheus Interface ---


struct PrometheusRequest {
    cluster: Cluster, //assume it's the one we're currently connected to? Try to get popeye info
    //from here?
    grouping: Option<Grouping>,
    resource: Resource,
    range: i64,
    time_scale: String,
}

impl PrometheusRequest {
    fn new(
        cluster: Cluster, //assume it's the one we're currently connected to? Try to get popeye info
        //from here?
        grouping: Option<Grouping>,
        resource: Resource,
        range: usize,
        time_scale: String,
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



enum PrometheusData {
    UsageData,
    CapacityData,
}

fn generic_data_request(request: PrometheusRequest, data_type: PrometheusData) -> Result<PrometheusData, AppError>{

    match data_type {
        PrometheusData::UsageData => {
            let data = get_usage_by(request.cluster, request.grouping.unwrap(), request.resource, request.range, &request.time_scale)
                .map_err(|e| AppError::DataFetch(e.to_string()))?;
            Ok(UsageData {source_data: data})
        },
        PrometheusData::CapacityData => {
            let data = get_max_resource(request.cluster, request.grouping, request.resource, request.range, &request.time_scale)
                .map_err(|e| AppError::DataFetch(e.to_string()))?;
            Ok(CapacityData {capacities: data})
        }
    }
    
}

// --- CPU by Account ---


// similarities: 
// we have functions which take in a usage request, and output data, either usage daata or capacity
// data
//
//
//


pub fn get_cpu_by_account_data() -> Result<UsageData, AppError> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d")
        .map_err(|e| AppError::DataFetch(e.to_string()))?;
    Ok(UsageData { source_data: data })
}

pub async fn get_cpu_by_account_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_by_account_data).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::CpuByAccount(data_res),
        Err(e) => FetchedData::CpuByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

pub fn get_cpu_capacity_by_account() -> Result<CapacityData, AppError> {
    let data = get_max_resource(
        Cluster::Rusty,
        Some(Grouping::Account),
        Resource::Cpus,
        Some(7),
        Some("1d"),
    )
    .map_err(|e| AppError::DataFetch(e.to_string()))?;

    Ok(CapacityData { capacities: data })
}

pub async fn get_cpu_capacity_by_account_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_capacity_by_account).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByAccount(data),
        Err(e) => FetchedData::CpuCapacityByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

// --- CPU by Node ---

pub fn get_cpu_by_node_data() -> Result<UsageData, AppError> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d")
        .map_err(|e| AppError::DataFetch(e.to_string()))?;
    Ok(UsageData { source_data: data })
}

pub async fn get_cpu_by_node_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_by_node_data).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::CpuByNode(data_res),
        Err(e) => FetchedData::CpuByNode(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

pub fn get_cpu_capacity_by_node() -> Result<CapacityData, AppError> {
    let data = get_max_resource(
        Cluster::Rusty,
        Some(Grouping::Nodes),
        Resource::Cpus,
        Some(7),
        Some("1d"),
    )
    .map_err(|e| AppError::DataFetch(e.to_string()))?;

    Ok(CapacityData { capacities: data })
}

pub async fn get_cpu_capacity_by_node_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_capacity_by_node).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByNode(data),
        Err(e) => FetchedData::CpuCapacityByNode(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

// --- GPU by Type ---

pub fn get_gpu_by_type_data() -> Result<UsageData, AppError> {
    let data = get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d")
        .map_err(|e| AppError::DataFetch(e.to_string()))?;
    Ok(UsageData { source_data: data })
}

pub async fn get_gpu_by_type_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_gpu_by_type_data).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::GpuByType(data_res),
        Err(e) => FetchedData::GpuByType(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

pub fn get_gpu_capacity_by_type() -> Result<CapacityData, AppError> {
    let data = get_max_resource(
        Cluster::Rusty,
        Some(Grouping::GpuType),
        Resource::Gpus,
        Some(7),
        Some("1d"),
    )
    .map_err(|e| AppError::DataFetch(e.to_string()))?;

    Ok(CapacityData { capacities: data })
}

pub async fn get_gpu_capacity_by_type_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_gpu_capacity_by_type).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::GpuCapacityByType(data),
        Err(e) => FetchedData::GpuCapacityByType(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {}
}

