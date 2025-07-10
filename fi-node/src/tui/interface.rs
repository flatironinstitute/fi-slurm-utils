use crate::tui::app::{AppError, ChartCapacity, ChartData, FetchedData};
use fi_prometheus::{get_max_resource, get_usage_by, Cluster, Grouping, Resource};
use tokio::sync::mpsc;

// --- Prometheus Interface ---

// --- CPU by Account ---

pub fn get_cpu_by_account_data() -> Result<ChartData, AppError> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d")
        .map_err(|e| AppError::DataFetch(e.to_string()))?;
    Ok(ChartData { source_data: data })
}

pub async fn get_cpu_by_account_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_by_account_data).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::CpuByAccount(data_res),
        Err(e) => FetchedData::CpuByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {
        // Error sending to main thread, it likely shut down.
    }
}

pub fn get_cpu_capacity_by_account() -> Result<ChartCapacity, AppError> {
    let data = get_max_resource(
        Cluster::Rusty,
        Some(Grouping::Account),
        Resource::Cpus,
        Some(7),
        Some("1d"),
    )
    .map_err(|e| AppError::DataFetch(e.to_string()))?;

    let max = *data.values().max().unwrap_or(&0);
    Ok(ChartCapacity {
        capacity_vec: data,
        max_capacity: max,
    })
}

pub async fn get_cpu_capacity_by_account_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_capacity_by_account).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByAccount(data),
        Err(e) => FetchedData::CpuCapacityByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {
        // Error sending to main thread, it likely shut down.
    }
}

// --- CPU by Node ---

pub fn get_cpu_by_node_data() -> Result<ChartData, AppError> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d")
        .map_err(|e| AppError::DataFetch(e.to_string()))?;
    Ok(ChartData { source_data: data })
}

pub async fn get_cpu_by_node_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_by_node_data).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::CpuByNode(data_res),
        Err(e) => FetchedData::CpuByNode(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {
        // Error sending to main thread, it likely shut down.
    }
}

pub fn get_cpu_capacity_by_node() -> Result<ChartCapacity, AppError> {
    let data = get_max_resource(
        Cluster::Rusty,
        Some(Grouping::Nodes),
        Resource::Cpus,
        Some(7),
        Some("1d"),
    )
    .map_err(|e| AppError::DataFetch(e.to_string()))?;

    let max = *data.values().max().unwrap_or(&0);
    Ok(ChartCapacity {
        capacity_vec: data,
        max_capacity: max,
    })
}

pub async fn get_cpu_capacity_by_node_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_cpu_capacity_by_node).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByNode(data),
        Err(e) => FetchedData::CpuCapacityByNode(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {
        // Error sending to main thread, it likely shut down.
    }
}

// --- GPU by Type ---

pub fn get_gpu_by_type_data() -> Result<ChartData, AppError> {
    let data = get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d")
        .map_err(|e| AppError::DataFetch(e.to_string()))?;
    Ok(ChartData { source_data: data })
}

pub async fn get_gpu_by_type_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_gpu_by_type_data).await;
    let data_to_send = match result {
        Ok(data_res) => FetchedData::GpuByType(data_res),
        Err(e) => FetchedData::GpuByType(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {
        // Error sending to main thread, it likely shut down.
    }
}

pub fn get_gpu_capacity_by_type() -> Result<ChartCapacity, AppError> {
    let data = get_max_resource(
        Cluster::Rusty,
        Some(Grouping::GpuType),
        Resource::Gpus, // Corrected from Cpus to Gpus
        Some(7),
        Some("1d"),
    )
    .map_err(|e| AppError::DataFetch(e.to_string()))?;

    let max = *data.values().max().unwrap_or(&0);
    Ok(ChartCapacity {
        capacity_vec: data,
        max_capacity: max,
    })
}

pub async fn get_gpu_capacity_by_type_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(get_gpu_capacity_by_type).await;
    let data_to_send = match result {
        Ok(data) => FetchedData::GpuCapacityByType(data),
        Err(e) => FetchedData::GpuCapacityByType(Err(AppError::TaskJoin(e.to_string()))),
    };
    if tx.send(data_to_send).await.is_err() {
        // Error sending to main thread, it likely shut down.
    }
}

