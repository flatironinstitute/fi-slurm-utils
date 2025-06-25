use std::{collections::HashMap, ffi::CStr, fmt};
use chrono::{DateTime, Utc};
use crate::utils::{time_t_to_datetime, c_str_to_string};
use crate::energy::AcctGatherEnergy; 
use crate::states::NodeStateFlags;
use rust_bind::bindings::{
    node_info_msg_t, node_info_t, 
    slurm_free_node_info_msg, slurm_load_node};

pub struct RawSlurmNodeInfo {
    ptr: *mut node_info_msg_t,
}

impl Drop for RawSlurmNodeInfo {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_free_node_info_msg(self.ptr);
            }
            // nullifying pointer after free, probably unnecessary but good form
            self.ptr = std::ptr::null_mut();
        }
    }
}

impl RawSlurmNodeInfo {
    pub fn load() -> Result<Self, String> {
        let mut node_info_msg_ptr: *mut node_info_msg_t = std::ptr::null_mut();

        let update_time = 0; // defaulting to time 0 to get all information

        let show_flags = 2; // only getting SHOW_DETAIL 

        let return_code = unsafe {
            slurm_load_node(
                update_time, 
                &mut node_info_msg_ptr, 
                show_flags)
        };

        if return_code != 0 || node_info_msg_ptr.is_null() {
            Err("Failed to load node information from Slurm".to_string())
        } else {
            Ok(RawSlurmNodeInfo { ptr: node_info_msg_ptr})
        }
    }

    pub fn as_slice(&self) -> &[node_info_t]{
        if self.ptr.is_null() {
            return &[];
        }

        unsafe {
            let msg = &*self.ptr;
            std::slice::from_raw_parts(msg.node_array, msg.record_count as usize)
        }
    }

    pub fn into_slurm_nodes(self) -> Result<SlurmNodes, String> {
        let raw_nodes_slice = self.as_slice();

        let nodes_map = raw_nodes_slice.iter().try_fold(HashMap::new(), |mut map, raw_node| {
            let safe_node = Node::from_raw_binding(raw_node)?;
            map.insert(safe_node.name.clone(), safe_node);
            Ok::<HashMap<String, Node>, String>(map)
        })?;

        let last_update_timestamp = unsafe { (*self.ptr).last_update };
        let last_update = chrono::DateTime::from_timestamp(last_update_timestamp, 0).unwrap_or_default();

        Ok(SlurmNodes {
            nodes: nodes_map,
            last_update,
        })
    }
}

pub fn get_nodes() -> Result<SlurmNodes, String> {
    // We load the raw C data into memory,
    // convert into safe, Rust-native structs, 
    // and then consume the wrapper to drop the original C memory
    RawSlurmNodeInfo::load()?.into_slurm_nodes()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeState {
    Allocated,
    Down,
    Error,
    Future,
    Idle, 
    Mixed, 
    Unknown(String),
    Compound {
        base: Box<NodeState>,
        flags: Vec<String>,
    },
    End,
}

impl From<u32> for NodeState {
    fn from(state_num: u32) -> Self {

        const NODE_STATE_DOWN: u32 = 1;
        const NODE_STATE_IDLE: u32 = 2;
        const NODE_STATE_ALLOCATED: u32 = 3;
        const NODE_STATE_ERROR: u32 = 4;
        const NODE_STATE_MIXED: u32 = 5;
        const NODE_STATE_FUTURE: u32 = 6;
        const NODE_STATE_END: u32 = 7;
        const BASE_STATE_MASK: u32 = 0xF;

        let base_state_num = state_num & BASE_STATE_MASK;
        let base_state = match base_state_num {
            NODE_STATE_DOWN => NodeState::Down,
            NODE_STATE_IDLE => NodeState::Idle,
            NODE_STATE_ALLOCATED => NodeState::Allocated,
            NODE_STATE_ERROR => NodeState::Error,
            NODE_STATE_MIXED => NodeState::Mixed,
            NODE_STATE_FUTURE => NodeState::Future,
            NODE_STATE_END=> NodeState::End,
            _ => NodeState::Unknown(format!("BASE({})", base_state_num)),
        };

        let flags_struct = NodeStateFlags::from_bits_truncate(state_num);

        let flags: Vec<String> = flags_struct
            .iter()
            .map(|flag| {
                // Get the verbose debug string, e.g., "NodeStateFlags(DRAIN | MAINT)"
                let debug_str = format!("{:?}", flag);
                // Extract just the name, e.g., "DRAIN"
                if let Some(start) = debug_str.find('(') {
                    if let Some(end) = debug_str.rfind(')') {
                        return debug_str[start + 1..end].to_string();
                    }
                }
                // Fallback for simple flags that might not have parentheses
                debug_str
            })
            .collect();
        
        if flags.is_empty() {
            // If no recognized flags are set, just return the base state
            base_state
        } else {
            // Otherwise, create a compound state
            NodeState::Compound {
                base: Box::new(base_state),
                flags,
            }
        }
    }
}

impl fmt::Display for NodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeState::Compound { base, flags } => {
                let flags_str = flags
                    .iter()
                    .map(|flag| flag.to_uppercase())
                    .collect::<Vec<String>>()
                    .join("+");
                write!(f, "{}+{}", base.to_string().to_uppercase(), flags_str)
            }
            NodeState::Unknown(s) => write!(f, "UNKNOWN({})", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Represents the GPU GRES of a node, assuming that a given node has only one kind of GPU
#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    pub total_gpus: u64,
    pub allocated_gpus: u64,
}

/// Parses gres and gres_used strings to create an optional GpuInfo struct
fn create_gpu_info(
    gres_str_ptr: *const i8,
    gres_used_ptr: *const i8,
) -> Option<GpuInfo> {
    /// A robust, local helper function to parse GRES strings
    fn parse_local_gres(raw_ptr: *const i8) -> HashMap<String, u64> {
        if raw_ptr.is_null() {
            return HashMap::new();
        }
        let gres_str = unsafe { CStr::from_ptr(raw_ptr) }.to_string_lossy();
        
        gres_str
            .split(',')
            .filter_map(|entry| {
                // First, strip off any parenthesized metadata like (IDX:...)
                let main_part = entry.split('(').next().unwrap_or(entry).trim();
                
                // Now, split the remaining "name:count" part.
                if let Some((key, count_str)) = main_part.rsplit_once(':') {
                    if let Ok(value) = count_str.parse::<u64>() {
                        Some((key.to_string(), value))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    let configured_map = parse_local_gres(gres_str_ptr);
    let allocated_map = parse_local_gres(gres_used_ptr);

    // Find the first (and likely only) GRES key that represents a GPU
    let gpu_key = configured_map
        .keys()
        .find(|key| key.starts_with("gpu"))
        .cloned()?; // Clone the key so we can use it for lookups

    let total_gpus = *configured_map.get(&gpu_key).unwrap_or(&0);
    let allocated_gpus = *allocated_map.get(&gpu_key).unwrap_or(&0);
    
    // Only create a GpuInfo struct if there are actually GPUs configured
    if total_gpus > 0 {
        Some(GpuInfo {
            name: gpu_key,
            total_gpus,
            allocated_gpus,
        })
    } else {
        None
    }
}


type NodeName = String;

// pub struct Node, a safe counterpart to node_info_t
#[derive(Debug, Clone)]
pub struct Node {
    pub name: NodeName,
    pub state: NodeState,
    pub next_state: NodeState,
    pub node_addr: String,
    pub node_hostname: String,

    //CPU Information
    pub cpus: u16,
    pub cores: u16,
    pub core_spec_count: u16,
    pub cpu_bind: u32,
    pub cpu_load: u32,
    pub cpus_effective: u16,
    pub cpu_spec_list: String,

    // Memory information (in MB)
    pub real_memory: u64,
    pub free_memory: u64,
    pub mem_spec_limit: u64,

    // Energy information
    _energy: Option<AcctGatherEnergy>,

    // Slurm Features 
    pub features: Vec<String>,
    pub active_features: Vec<String>, // aka features_act

    // Generic Resources (GRES), like GPUs
    // pub configured_gres: HashMap<String, u64>,
    // pub allocated_gres: HashMap<String, u64>,
    pub gpu_info: Option<GpuInfo>,
    pub gres: String,
    pub gres_drain: String,
    pub gres_used: String,
    pub res_cores_per_gpu: u16,
    pub gpu_spec: String,

    // Time information
    pub boot_time: DateTime<Utc>, // converted from a C i64 time_t
    pub last_busy: DateTime<Utc>,
    pub slurmd_start_time: DateTime<Utc>,

    // Other
    pub architecture: String,
    pub operating_system: String,
    pub reason: String,
    pub broadcast_address: String,
    pub boards: u16,
    pub cluster_name: String,
    pub extra: String,
    pub instance_id: String,
    pub instance_type: String,
    pub mcs_label: String,
    pub os: String,
    pub owner: u32,
    pub partitions: String,
    pub port: u16,
    pub comment: String,
    pub reason_time: DateTime<Utc>,
    pub reason_uid: u32,
    pub resume_after: DateTime<Utc>,
    pub resv_name: String,
    // pub select_nodeinfo: *mut bindings::dynamic_plugin_data, // replace with safe, owned version
    // null-initialized pointer to dynamic plugin data. Leaving commented for the moment unless we
    // turn out to need it
    pub sockets: u16,
    pub threads: u16,
    pub tmp_disk: u32,
    pub weight: u32,
    pub tres_fmt_str: String,
    pub version: String,
}

impl Node {
    /// Creates a safe, owned Rust `Node` from a raw C-style `node_info_t` struct.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the `raw_node` contains valid pointers
    /// for all string fields, as provided by a trusted Slurm API call
    pub fn from_raw_binding(raw_node: &node_info_t) -> Result<Self, String> {

        // Helper to convert comma-separated C string to a Vec<String>
        let c_str_to_vec = |ptr: *const i8| -> Vec<String> {
            if ptr.is_null() {
                Vec::new()
            } else {
                let r_str = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
                r_str.split(',').map(String::from).collect()
            }
        };

        let energy = if raw_node.energy.is_null() {
            None
        } else {
            Some(AcctGatherEnergy::from_raw_binding(unsafe {&*raw_node.energy})?)
        };

        // used as a sentinel value in C, replace when we have proper enums set up
        const NODE_STATE_END: u32 = 7;
        let next_state_val = if raw_node.next_state == NODE_STATE_END {
            NodeState::Unknown("N/A".to_string())
        } else {
            NodeState::from(raw_node.next_state)
        };


        Ok(Node {
            // Basic identification
            name: unsafe {c_str_to_string(raw_node.name)},
            state: NodeState::from(raw_node.node_state), // Directly convert the u32 state
            next_state: next_state_val,
            node_addr: unsafe {c_str_to_string(raw_node.node_addr)},
            node_hostname: unsafe {c_str_to_string(raw_node.node_hostname)},

            // CPU Information
            cpus: raw_node.cpus,
            cores: raw_node.cores,
            core_spec_count: raw_node.core_spec_cnt,
            cpu_bind: raw_node.cpu_bind,
            cpu_load: raw_node.cpu_load,
            cpus_effective: raw_node.cpus_efctv,
            cpu_spec_list: "TODO: Implement cpu_spec_list parsing".to_string(), // Placeholder

            // Memory information (in MB)
            real_memory: raw_node.real_memory,
            free_memory: raw_node.free_mem, 
            mem_spec_limit: raw_node.mem_spec_limit,

            _energy: energy,

            // Slurm Features
            features: c_str_to_vec(raw_node.features),
            active_features: c_str_to_vec(raw_node.features_act),


            // Generic Resources (GRES)
            gpu_info: create_gpu_info(raw_node.gres, raw_node.gres_used),
            gres: unsafe {c_str_to_string(raw_node.gres)}, // Keep the raw string for reference
            gres_drain: unsafe {c_str_to_string(raw_node.gres_drain)},
            gres_used: unsafe {c_str_to_string(raw_node.gres_used)}, // Keep the raw string for reference
            res_cores_per_gpu: raw_node.res_cores_per_gpu,
            gpu_spec: "TODO: Implement gpu_spec parsing".to_string(), // Placeholder

            // Time information
            boot_time: time_t_to_datetime(raw_node.boot_time),
            last_busy: time_t_to_datetime(raw_node.last_busy),
            slurmd_start_time: time_t_to_datetime(raw_node.slurmd_start_time),
            reason_time: time_t_to_datetime(raw_node.reason_time),
            resume_after: time_t_to_datetime(raw_node.resume_after),

            // Other
            architecture: unsafe {c_str_to_string(raw_node.arch)},
            operating_system: unsafe {c_str_to_string(raw_node.os)},
            reason: unsafe {c_str_to_string(raw_node.reason)},
            broadcast_address: unsafe {c_str_to_string(raw_node.bcast_address)},
            boards: raw_node.boards,
            cluster_name: unsafe {c_str_to_string(raw_node.cluster_name)},
            extra: unsafe {c_str_to_string(raw_node.extra)},
            comment: unsafe {c_str_to_string(raw_node.comment)},
            instance_id: "TODO".to_string(),  // These fields may not have direct mappings
            instance_type: "TODO".to_string(),
            mcs_label: unsafe {c_str_to_string(raw_node.mcs_label)},
            os: unsafe {c_str_to_string(raw_node.os)}, // Duplicate of operating_system? Included for completeness.
            owner: raw_node.owner,
            partitions: unsafe {c_str_to_string(raw_node.partitions)},
            port: raw_node.port,
            reason_uid: raw_node.reason_uid,
            resv_name: unsafe {c_str_to_string(raw_node.resv_name)},

            // TODO: `select_nodeinfo` is a void pointer to plugin-specific data
            // Handling this requires knowing which select plugin is active and how
            // to interpret its data structure
            // For now, we will ignore it
            // select_nodeinfo: ...,
            
            sockets: raw_node.sockets,
            threads: raw_node.threads,
            tmp_disk: raw_node.tmp_disk,
            weight: raw_node.weight,
            tres_fmt_str: "TODO: Parse TRES format string".to_string(), // Placeholder
            version: unsafe {c_str_to_string(raw_node.version)},
        })
    }
}


#[derive(Debug, Clone)]
pub struct SlurmNodes {
    pub nodes: std::collections::HashMap<String, Node>,
    pub last_update: DateTime<Utc>,
}
