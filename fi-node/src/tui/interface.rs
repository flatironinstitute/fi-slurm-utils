use crate::tui::app::AppError;
use tokio::sync::mpsc;
use fi_prometheus::{Cluster, Resource, Grouping, get_usage_by, get_max_resource};
use crate::tui::app::{
    ChartData, 
    FetchedData, 
    ChartCapacity,
};
// --- Prometheus interface ---

// Prometheus interface 

pub fn get_cpu_by_account_data() -> ChartData {
    let data = get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d").unwrap_or_default();
    
    ChartData {
        source_data: data,
    }
}

pub async fn get_cpu_by_account_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_by_account_data()
    }).await;

    let data_to_send = match result {
        Ok(data) => FetchedData::CpuByAccount(Ok(data)),
        Err(e) => FetchedData::CpuByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };

    if tx.send(data_to_send).await.is_err() {
        // error sending to main thread, take no action
    }
}

pub fn get_cpu_capacity_by_account() -> ChartCapacity {
    let data = get_max_resource(Cluster::Rusty, Some(Grouping::Account), Resource::Cpus, Some(7), Some("1d")).unwrap_or_default();

    let binding = data.clone();
    let max = &binding.values().max().unwrap_or(&0);

    ChartCapacity {
        capacity_vec: data,
        max_capacity: **max,
    }
}

pub async fn get_cpu_capacity_by_account_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_capacity_by_account()
    }).await;

    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByAccount(Ok(data)),
        Err(e) => FetchedData::CpuCapacityByAccount(Err(AppError::TaskJoin(e.to_string()))),
    };

    if tx.send(data_to_send).await.is_err() {
        // error sending to main thread, take no action
    }
}

pub fn get_cpu_by_node_data() -> ChartData {
    let data = get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d").unwrap_or_default();

    ChartData {
        source_data: data,
    }
}

pub async fn get_cpu_by_node_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_by_node_data()
    }).await;

    let data_to_send = match result {
        Ok(data) => FetchedData::CpuByNode(Ok(data)),
        Err(e) => FetchedData::CpuByNode(Err(AppError::TaskJoin(e.to_string()))),
    };

    if tx.send(data_to_send).await.is_err() {
        // error sending to main thread, take no action
    }
}

pub fn get_cpu_capacity_by_node() -> ChartCapacity {
    let data = get_max_resource(Cluster::Rusty, Some(Grouping::Nodes), Resource::Cpus, Some(7), Some("1d")).unwrap_or_default();

    let binding = data.clone();
    let max = &binding.values().max().unwrap_or(&0);

    ChartCapacity {
        capacity_vec: data,
        max_capacity: **max,
    }
}

pub async fn get_cpu_capacity_by_node_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_capacity_by_node()
    }).await;

    let data_to_send = match result {
        Ok(data) => FetchedData::CpuCapacityByNode(Ok(data)),
        Err(e) => FetchedData::CpuCapacityByNode(Err(AppError::TaskJoin(e.to_string()))),
    };

    if tx.send(data_to_send).await.is_err() {
        // error sending to main thread, take no action
    }
}

pub fn get_gpu_by_type_data() -> ChartData {
    let data = get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d").unwrap_or_default();
    
    ChartData {
        source_data: data,
    }
}

pub async fn get_gpu_by_type_data_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(move || {
        get_gpu_by_type_data()
    }).await;

    let data_to_send = match result {
        Ok(data) => FetchedData::GpuByType(Ok(data)),
        Err(e) => FetchedData::GpuByType(Err(AppError::TaskJoin(e.to_string()))),
    };

    if tx.send(data_to_send).await.is_err() {
        // error sending to main thread, take no action
    }
}

pub fn get_gpu_capacity_by_type() -> ChartCapacity {
    let data = get_max_resource(Cluster::Rusty, Some(Grouping::GpuType), Resource::Cpus, Some(7), Some("1d")).unwrap_or_default();

    let binding = data.clone();
    let max = &binding.values().max().unwrap_or(&0);

    ChartCapacity {
        capacity_vec: data,
        max_capacity: **max,
    }
}

pub async fn get_gpu_capacity_by_type_async(tx: mpsc::Sender<FetchedData>) {
    let result = tokio::task::spawn_blocking(move || {
        get_gpu_capacity_by_type()
    }).await;

    let data_to_send = match result {
        Ok(data) => FetchedData::GpuCapacityByType(Ok(data)),
        Err(e) => FetchedData::GpuCapacityByType(Err(AppError::TaskJoin(e.to_string()))),
    };

    if tx.send(data_to_send).await.is_err() {
        // error sending to main thread, take no action
    }
}

