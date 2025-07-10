use tokio::sync::mpsc;
use fi_prometheus::{Cluster, Resource, Grouping, get_usage_by, get_max_resource};
use crate::tui::app::{
    ChartData, 
    FetchedData, 
    ChartCapacity,
    FetchedCapacity,
};
// --- Prometheus interface ---

// Prometheus interface 

pub fn get_cpu_by_account_data<'a>() -> ChartData<'a> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d").unwrap_or_default();

    let binding = data.clone();
    let max = binding.values().map(|vec| vec.iter().sum::<u64>()).max().unwrap_or(0);
    
    ChartData {
        _title: "CPU Usage by Account (8 Days)",
        source_data: data,
        _y_axis_bounds: [0.0, max as f64],
        _y_axis_title: "CPU Cores",
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

pub async fn get_cpu_capacity_by_account_async(tx: mpsc::Sender<FetchedCapacity>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_capacity_by_account()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedCapacity::CpuByAccount(data)).await.is_err() {
            // Handle error: receiver was dropped.
        }
    }
}

pub fn get_cpu_by_node_data<'a>() -> ChartData<'a> {
    let data = get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d").unwrap_or_default();
    
    let binding = data.clone();
    let max = binding.values().map(|vec| vec.iter().sum::<u64>()).max().unwrap_or(0);

    ChartData {
        _title: "CPU Usage by Node Type (8 Days)",
        source_data: data,
        _y_axis_bounds: [0.0, max as f64],
        _y_axis_title: "CPU Cores",
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

pub fn get_gpu_by_type_data<'a>() -> ChartData<'a> {
    let data = get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d").unwrap_or_default();
    
    let binding = data.clone();
    let max = binding.values().map(|vec| vec.iter().sum::<u64>()).max().unwrap_or(0);

    ChartData {
        _title: "GPU Usage by Type (8 Days)",
        source_data: data,
        _y_axis_bounds: [0.0, max as f64],
        _y_axis_title: "GPUs",
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


pub async fn get_cpu_by_account_data_async(tx: mpsc::Sender<FetchedData<'_>>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_by_account_data()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedData::CpuByAccount(Ok(data))).await.is_err() {
            // Handle error: receiver was dropped.
        }
    }
}

pub async fn get_cpu_by_node_data_async(tx: mpsc::Sender<FetchedData<'_>>) {
    let result = tokio::task::spawn_blocking(move || {
        get_cpu_by_node_data()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedData::CpuByNode(Ok(data))).await.is_err() {
            // Handle error
        }
    }
}

pub async fn get_gpu_by_type_data_async(tx: mpsc::Sender<FetchedData<'_>>) {
    let result = tokio::task::spawn_blocking(move || {
        get_gpu_by_type_data()
    }).await;

    if let Ok(data) = result {
        if tx.send(FetchedData::GpuByType(Ok(data))).await.is_err() {
            // Handle error
        }
    }
}
