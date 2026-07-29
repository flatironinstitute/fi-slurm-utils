//! Reads the controller's association manager, which holds the usage counters Slurm
//! enforces limits against.
//!
//! Usage here is authoritative in a way that counting job records is not: it is per QOS
//! rather than per partition, so it is already summed over every partition sharing a QOS,
//! and it counts what the scheduler counts.

use crate::list::{SlurmIterator, vec_to_slurm_list};
use crate::utils::c_str_to_string;
use fi_slurm_sys::{
    ASSOC_MGR_INFO_FLAG_QOS, assoc_mgr_info_msg_t, assoc_mgr_info_request_msg_t,
    slurm_free_assoc_mgr_info_msg, slurm_list_destroy, slurm_load_assoc_mgr_info,
    slurmdb_qos_rec_t, slurmdb_used_limits_t,
};
use std::collections::HashMap;

/// Owns the request struct and the Slurm lists inside it
struct AssocMgrRequest {
    ptr: *mut assoc_mgr_info_request_msg_t,
}

impl AssocMgrRequest {
    /// Asks only for QOS records, and only those the given users appear in
    fn new(users: Vec<String>) -> Self {
        let mut req: assoc_mgr_info_request_msg_t = unsafe { std::mem::zeroed() };
        req.flags = ASSOC_MGR_INFO_FLAG_QOS;
        req.user_list = unsafe { vec_to_slurm_list(Some(users)) };

        Self {
            ptr: Box::into_raw(Box::new(req)),
        }
    }
}

impl Drop for AssocMgrRequest {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                let req: &mut assoc_mgr_info_request_msg_t = &mut *self.ptr;
                for list in [req.acct_list, req.qos_list, req.user_list] {
                    if !list.is_null() {
                        slurm_list_destroy(list);
                    }
                }
                let _ = Box::from_raw(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

/// Owns the reply, which Slurm allocated
struct RawAssocMgrInfo {
    ptr: *mut assoc_mgr_info_msg_t,
}

impl Drop for RawAssocMgrInfo {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_free_assoc_mgr_info_msg(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

/// What one user has running against one QOS, as the controller counts it
#[derive(Debug, Clone, Default)]
pub struct UserUsage {
    pub jobs: u32,
    pub submitted_jobs: u32,
    /// Keyed by TRES name as the controller names it, e.g. "cpu", "node", "gres/gpu"
    pub tres: HashMap<String, u64>,
}

/// One QOS, with the usage counted against it
#[derive(Debug, Clone)]
pub struct QosUsage {
    pub name: String,
    /// Usage by everyone in the QOS, which is what GrpTRES limits are measured against
    pub group: UserUsage,
    /// Usage per user, which is what MaxTRESPU and MaxJobsPU are measured against
    pub per_user: HashMap<u32, UserUsage>,
}

/// Reads the TRES counter array, which is indexed by the position of each name in
/// `tres_names`, dropping the zeroes so callers only see what is in use
unsafe fn tres_map(counts: *mut u64, names: &[String]) -> HashMap<String, u64> {
    if counts.is_null() || names.is_empty() {
        return HashMap::new();
    }

    let counts = unsafe { std::slice::from_raw_parts(counts, names.len()) };
    names
        .iter()
        .zip(counts)
        .filter(|&(_, &count)| count != 0)
        .map(|(name, &count)| (name.clone(), count))
        .collect()
}

/// Fetches the QOS usage counters for `users` from the controller
pub fn get_qos_usage(users: Vec<String>) -> Result<Vec<QosUsage>, String> {
    let request = AssocMgrRequest::new(users);
    let mut resp_ptr: *mut assoc_mgr_info_msg_t = std::ptr::null_mut();

    let rc = unsafe { slurm_load_assoc_mgr_info(request.ptr, &mut resp_ptr) };
    if rc != 0 || resp_ptr.is_null() {
        return Err("Failed to load association manager info from Slurm".to_string());
    }
    let resp = RawAssocMgrInfo { ptr: resp_ptr };

    // Safety: the reply owns both lists for as long as `resp` is alive
    let msg = unsafe { &*resp.ptr };

    let tres_names: Vec<String> = if msg.tres_names.is_null() {
        Vec::new()
    } else {
        let raw = unsafe { std::slice::from_raw_parts(msg.tres_names, msg.tres_cnt as usize) };
        raw.iter()
            .map(|&name| unsafe { c_str_to_string(name) })
            .collect()
    };

    let qos = unsafe { SlurmIterator::new(msg.qos_list) }
        .map(|rec| {
            let rec = rec as *const slurmdb_qos_rec_t;
            unsafe { read_qos(rec, &tres_names) }
        })
        .collect();

    Ok(qos)
}

/// # Safety
/// `rec` must point to a valid QOS record from an association manager reply.
unsafe fn read_qos(rec: *const slurmdb_qos_rec_t, tres_names: &[String]) -> QosUsage {
    let name = unsafe { c_str_to_string((*rec).name) };
    let usage = unsafe { (*rec).usage };

    if usage.is_null() {
        return QosUsage {
            name,
            group: UserUsage::default(),
            per_user: HashMap::new(),
        };
    }

    let group = UserUsage {
        jobs: unsafe { (*usage).grp_used_jobs },
        submitted_jobs: unsafe { (*usage).grp_used_submit_jobs },
        tres: unsafe { tres_map((*usage).grp_used_tres, tres_names) },
    };

    let per_user = unsafe { SlurmIterator::new((*usage).user_limit_list) }
        .map(|entry| {
            let limits = entry as *const slurmdb_used_limits_t;
            let usage = UserUsage {
                jobs: unsafe { (*limits).jobs },
                submitted_jobs: unsafe { (*limits).submit_jobs },
                tres: unsafe { tres_map((*limits).tres, tres_names) },
            };
            (unsafe { (*limits).uid }, usage)
        })
        .collect();

    QosUsage {
        name,
        group,
        per_user,
    }
}
