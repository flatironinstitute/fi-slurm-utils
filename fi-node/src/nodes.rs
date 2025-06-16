use std::{collections::HashMap, ffi::CStr, fmt};
use chrono::{DateTime, Utc};

use crate::{bindings::{node_info_msg_t, node_info_t, slurm_free_node_info_msg, slurm_load_node}, energy::AcctGatherEnergy, gres::parse_gres_str};

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

        let show_flags = 0; //placeholder for more detailed flag behavior

        let return_code = unsafe {
            slurm_load_node(
                update_time, 
                &mut node_info_msg_ptr, // passing mutable reference here
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

        // let mut nodes_map = std::collections::HashMap::new();
        let nodes_map = raw_nodes_slice.iter().try_fold(HashMap::new(), |mut map, raw_node| {
            let safe_node = Node::from_raw_binding(raw_node)?;
            map.insert(safe_node.name.clone(), safe_node);
            Ok::<HashMap<String, Node>, String>(map)
        })?;

        let last_update_timestamp = unsafe { (*self.ptr).last_update };
        let _last_update = chrono::DateTime::from_timestamp(last_update_timestamp, 0).unwrap_or_default();
        // Utc.from_utc_datetime(&NaiveDateTime::from_timestamp_opt(last_update_timestamp, 0).unwrap_or_default(),);

        Ok(SlurmNodes {
            nodes: nodes_map,
            _last_update,
        })
    }

}

pub fn get_nodes() -> Result<SlurmNodes, String> {
    // This single line encapsulates the entire process:
    // 1. Load the raw C data into our RAII wrapper.
    // 2. Consume the wrapper to convert the data into our final, safe collection.
    // The `?` operator will propagate any errors from either step.
    RawSlurmNodeInfo::load()?.into_slurm_nodes()
}

// pub const node_states_NODE_STATE_UNKNOWN: node_states = 0;
// pub const node_states_NODE_STATE_DOWN: node_states = 1;
// pub const node_states_NODE_STATE_IDLE: node_states = 2;
// pub const node_states_NODE_STATE_ALLOCATED: node_states = 3;
// pub const node_states_NODE_STATE_ERROR: node_states = 4;
// pub const node_states_NODE_STATE_MIXED: node_states = 5;
// pub const node_states_NODE_STATE_FUTURE: node_states = 6;
// pub const node_states_NODE_STATE_END: node_states = 7;
// pub type node_states = ::std::os::raw::c_uint;

// interestingly, DRAIN is not a node state. Look into that
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

// NOTE: This new `impl From<u32> for NodeState` block should be added to your nodes.rs file.
// It maps the integer constants from C into our safe Rust enum.
#[allow(dead_code)]
impl From<u32> for NodeState {
    fn from(state_num: u32) -> Self {
        // These constants must match the values in your bindings.rs file.
        // e.g., bindings::node_states_NODE_STATE_IDLE
        const NODE_STATE_UNKNOWN: u32 = 0;
        const NODE_STATE_DOWN: u32 = 1;
        const NODE_STATE_IDLE: u32 = 2;
        const NODE_STATE_ALLOCATED: u32 = 3;
        const NODE_STATE_ERROR: u32 = 4;
        const NODE_STATE_MIXED: u32 = 5;
        const NODE_STATE_FUTURE: u32 = 6;
        const NODE_STATE_END: u32 = 7;

        const NODE_STATE_EXTERNAL: u32 = 1 << 4;
        const NODE_STATE_RES: u32 = 1 << 5;
        const NODE_STATE_UNDRAIN: u32 = 1 << 6;
        const NODE_STATE_CLOUD: u32 = 1 << 7;
        const NODE_STATE_RESUME: u32 = 1 << 8;
        const NODE_STATE_DRAIN: u32 = 1 << 9;
        const NODE_STATE_COMPLETING: u32 = 1 << 10;
        const NODE_STATE_NO_RESPOND: u32 = 1 << 11;
        const NODE_STATE_POWERED_DOWN: u32 = 1 << 12;
        const NODE_STATE_FAIL: u32 = 1 << 13;
        const NODE_STATE_POWERING_UP: u32 = 1 << 14;
        const NODE_STATE_MAINT: u32 = 1 << 15;
        const NODE_STATE_REBOOT_REQUESTED: u32 = 1 << 16;
        const NODE_STATE_REBOOT_CANCEL: u32 = 1 << 17;
        const NODE_STATE_POWERING_DOWN: u32 = 1 << 18;
        const NODE_STATE_DYNAMIC_FUTURE: u32 = 1 << 19;
        const NODE_STATE_REBOOT_ISSUED: u32 = 1 << 20;
        const NODE_STATE_PLANNED: u32 = 1 << 21;
        const NODE_STATE_INVALID_REG: u32 = 1 << 22;
        const NODE_STATE_POWER_DOWN: u32 = 1 << 23;
        const NODE_STATE_POWER_UP: u32 = 1 << 24;
        const NODE_STATE_POWER_DRAIN: u32 = 1 << 25;
        const NODE_STATE_DYNAMIC_NORM: u32 = 1 << 26;
        const NODE_STATE_BLOCKED: u32 = 1 << 27;


        // The bitmask for the base state (values 0-15, which is 0x0F or 0b1111)
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

        let mut flags: Vec<String> = Vec::new();


        // Check for each flag we care about.
        if (state_num & NODE_STATE_EXTERNAL) != 0 {
            flags.push("EXTERNAL".to_string());
        }
        if (state_num & NODE_STATE_RES) != 0 {
            flags.push("RES".to_string());
        }
        if (state_num & NODE_STATE_UNDRAIN) != 0 {
            flags.push("UNDRAIN".to_string());
        }
        if (state_num & NODE_STATE_CLOUD) != 0 {
            flags.push("CLOUD".to_string());
        }
        if (state_num & NODE_STATE_RESUME) != 0 {
            flags.push("RESUME".to_string());
        }
        if (state_num & NODE_STATE_DRAIN) != 0 {
            flags.push("DRAIN".to_string());
        }
        if (state_num & NODE_STATE_COMPLETING) != 0 {
            flags.push("COMPLETING".to_string());
        }
        if (state_num & NODE_STATE_NO_RESPOND) != 0 {
            flags.push("NO_RESPOND".to_string());
        }
        if (state_num & NODE_STATE_POWERED_DOWN) != 0 {
            flags.push("POWERED_DOWN".to_string());
        }
        if (state_num & NODE_STATE_FAIL) != 0 {
            flags.push("FAIL".to_string());
        }
        if (state_num & NODE_STATE_POWERING_UP) != 0 {
            flags.push("POWERING_UP".to_string());
        }
        if (state_num & NODE_STATE_MAINT) != 0 {
            flags.push("MAINT".to_string());
        }
        if (state_num & NODE_STATE_REBOOT_REQUESTED) != 0 {
            flags.push("REBOOT_REQUESTED".to_string());
        }
        if (state_num & NODE_STATE_REBOOT_CANCEL) != 0 {
            flags.push("REBOOT_CANCEL".to_string());
        }
        if (state_num & NODE_STATE_POWERING_DOWN) != 0 {
            flags.push("POWERING_DOWN".to_string());
        }
        if (state_num & NODE_STATE_DYNAMIC_FUTURE) != 0 {
            flags.push("DYNAMIC_FUTURE".to_string());
        }
        if (state_num & NODE_STATE_REBOOT_ISSUED) != 0 {
            flags.push("REBOOT_ISSUED".to_string());
        }
        if (state_num & NODE_STATE_PLANNED) != 0 {
            flags.push("REBOOT_PLANNED".to_string());
        }
        if (state_num & NODE_STATE_INVALID_REG) != 0 {
            flags.push("INVALID_REG".to_string());
        }
        if (state_num & NODE_STATE_POWER_DOWN) != 0 {
            flags.push("POWER_DOWN".to_string());
        }
        if (state_num & NODE_STATE_POWER_UP) != 0 {
            flags.push("POWER_UP".to_string());
        }
        if (state_num & NODE_STATE_POWER_DRAIN) != 0 {
            flags.push("POWER_DRAIN".to_string());
        }
        if (state_num & NODE_STATE_DYNAMIC_NORM) != 0 {
            flags.push("DYNAMIC_NORM".to_string());
        }
        if (state_num & NODE_STATE_BLOCKED) != 0 {
            flags.push("BLOCKED".to_string());
        }
        
        if flags.is_empty() {
            // If no recognized flags are set, just return the base state.
            base_state
        } else {
            // Otherwise, create a compound state.
            NodeState::Compound {
                base: Box::new(base_state),
                flags,
            }
        }


        // let flags_num = state_num & !BASE_STATE_MASK;
        //
        // if flags_num == 0 {
        //     // ie if no flags are set, we just return the base state
        //     base_state
        // } else {
        //     let mut detected_flags = Vec::new();
        //     let mut temp_flags = flags_num;
        //     let mut bit = BASE_STATE_MASK + 1;
        //
        //     while temp_flags > 0 {
        //         if (temp_flags & bit) != 0 {
        //             detected_flags.push(bit);
        //             temp_flags &= !bit;
        //         }
        //         if bit == 0 {break;}
        //         bit <<= 1;
        //     }
        //
        //     NodeState::Compound {
        //         base: Box::new(base_state),
        //         flags: detected_flags,
        //     }
        // }

        //match state_num {
        //    NODE_STATE_UNKNOWN => NodeState::Unknown("UNKNOWN".to_string()),
        //    NODE_STATE_DOWN => NodeState::Down,
        //    NODE_STATE_IDLE => NodeState::Idle,
        //    NODE_STATE_ALLOCATED => NodeState::Allocated,
        //    NODE_STATE_ERROR => NodeState::Unknown("ERROR".to_string()), // Or a dedicated Error variant
        //    NODE_STATE_MIXED => NodeState::Mixed,
        //    NODE_STATE_FUTURE => NodeState::Unknown("FUTURE".to_string()), // Or a dedicated Future variant
        //    _ => NodeState::Unknown(format!("Untracked State Code: {}", state_num)),
        //}
    }
}

impl fmt::Display for NodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeState::Compound { base, flags } => {
                // Creates strings like "IDLE+DRAIN"
                write!(f, "{}+{}", base, flags.join("+"))
            }
            NodeState::Unknown(s) => write!(f, "UNKNOWN({})", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

// pub struct Node, a safe counterpart to node_info_t
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub state: NodeState,
    pub next_state: NodeState, // double check
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

    // Generic Resources (GRES), like GPUs. We can model this as a map.
    // e.g., {"gpu": 4, "license_A": 8}
    pub configured_gres: HashMap<String, u64>,
    pub allocated_gres: HashMap<String, u64>,
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
    /// for all string fields, as provided by a trusted Slurm API call.
    pub fn from_raw_binding(raw_node: &node_info_t) -> Result<Self, String> {
        // Helper function to safely convert a C string pointer to an owned Rust String.
        // Returns an empty string if the pointer is null.
        let c_str_to_string = |ptr: *const i8| -> String {
            if ptr.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
            }
        };

        // Helper to convert comma-separated C string to a Vec<String>.
        let c_str_to_vec = |ptr: *const i8| -> Vec<String> {
            if ptr.is_null() {
                Vec::new()
            } else {
                let r_str = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
                r_str.split(',').map(String::from).collect()
            }
        };

        // Helper to convert time_t (i64) to DateTime<Utc>.
        let time_t_to_datetime = |timestamp: i64| -> DateTime<Utc> {
            // Create a NaiveDateTime and then convert to UTC DateTime.
            // Using unwrap_or_default for robustness against out-of-range timestamps.
           chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_default()

            //Utc.from_utc_datetime(
            //    &chrono::NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap_or_default(),
            //)
        };

        let energy = if raw_node.energy.is_null() {
            None
        } else {
            Some(AcctGatherEnergy::from_raw_binding(unsafe {&*raw_node.energy})?)
        };

        // Per sysadmin, the END state (7) is used as the sentinel for "not set".
        // revisit this along with the other enums to make them more robust
        const NODE_STATE_END: u32 = 7;
        let next_state_val = if raw_node.next_state == NODE_STATE_END {
             // We can use a special variant or just map to Unknown.
            NodeState::Unknown("N/A".to_string())
        } else {
            NodeState::from(raw_node.next_state)
        };


        Ok(Node {
            // Basic identification
            name: c_str_to_string(raw_node.name),
            state: NodeState::from(raw_node.node_state), // Directly convert the u32 state
            next_state: next_state_val,
            //state: NodeState::from("TODO: Get actual state string"), // Placeholder: This needs the full state string.
            //next_state: NodeState::from("TODO: Get next state string"), // Placeholder for next_state.
            node_addr: c_str_to_string(raw_node.node_addr),
            node_hostname: c_str_to_string(raw_node.node_hostname),

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
            free_memory: raw_node.free_mem, // Assuming this is the correct field
            mem_spec_limit: raw_node.mem_spec_limit,

            _energy: energy,

            // Slurm Features
            features: c_str_to_vec(raw_node.features),
            active_features: c_str_to_vec(raw_node.features_act),


            // Generic Resources (GRES)
            configured_gres: unsafe {parse_gres_str(raw_node.gres)},
            allocated_gres: unsafe {parse_gres_str(raw_node.gres_used)},
            gres: c_str_to_string(raw_node.gres), // Keep the raw string for reference
            gres_drain: c_str_to_string(raw_node.gres_drain),
            gres_used: c_str_to_string(raw_node.gres_used), // Keep the raw string for reference
            res_cores_per_gpu: raw_node.res_cores_per_gpu, // Assuming this is correct field
            gpu_spec: "TODO: Implement gpu_spec parsing".to_string(), // Placeholder

            // Time information
            boot_time: time_t_to_datetime(raw_node.boot_time),
            last_busy: time_t_to_datetime(raw_node.last_busy),
            slurmd_start_time: time_t_to_datetime(raw_node.slurmd_start_time),
            reason_time: time_t_to_datetime(raw_node.reason_time),
            resume_after: time_t_to_datetime(raw_node.resume_after),

            // Other
            architecture: c_str_to_string(raw_node.arch),
            operating_system: c_str_to_string(raw_node.os),
            reason: c_str_to_string(raw_node.reason),
            broadcast_address: c_str_to_string(raw_node.bcast_address),
            boards: raw_node.boards,
            cluster_name: c_str_to_string(raw_node.cluster_name),
            extra: c_str_to_string(raw_node.extra),
            comment: c_str_to_string(raw_node.comment),
            instance_id: "TODO".to_string(),  // These fields may not have direct mappings
            instance_type: "TODO".to_string(),
            mcs_label: c_str_to_string(raw_node.mcs_label),
            os: c_str_to_string(raw_node.os), // Duplicate of operating_system? Included for completeness.
            owner: raw_node.owner,
            partitions: c_str_to_string(raw_node.partitions),
            port: raw_node.port,
            reason_uid: raw_node.reason_uid,
            resv_name: c_str_to_string(raw_node.resv_name),

            // TODO: `select_nodeinfo` is a void pointer to plugin-specific data.
            // Handling this requires knowing which select plugin is active and how
            // to interpret its data structure. This is a very advanced topic.
            // For now, we will ignore it.
            // select_nodeinfo: ...,
            
            sockets: raw_node.sockets,
            threads: raw_node.threads,
            tmp_disk: raw_node.tmp_disk,
            weight: raw_node.weight,
            tres_fmt_str: "TODO: Parse TRES format string".to_string(), // Placeholder
            version: c_str_to_string(raw_node.version),
        })
    }
}


#[derive(Debug, Clone)]
pub struct SlurmNodes {
    pub nodes: std::collections::HashMap<String, Node>,
    _last_update: DateTime<Utc>,
}
