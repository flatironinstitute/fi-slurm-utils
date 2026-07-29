//! Reads the controller's association manager, which holds the usage counters Slurm
//! enforces limits against.
//!
//! Usage here is authoritative in a way that counting job records is not: it is per QOS
//! rather than per partition, so it is already summed over every partition sharing a QOS,
//! and it counts what the scheduler counts.

use crate::list::{SlurmIterator, vec_to_slurm_list};
use crate::utils::c_str_to_string;
use fi_slurm_sys::{
    ASSOC_MGR_INFO_FLAG_QOS, ASSOC_MGR_INFO_FLAG_USERS, assoc_mgr_info_msg_t,
    assoc_mgr_info_request_msg_t, slurm_free_assoc_mgr_info_msg, slurm_list_destroy,
    slurm_load_assoc_mgr_info, slurmdb_qos_rec_t, slurmdb_used_limits_t, slurmdb_user_rec_t,
};
use std::collections::HashMap;

/// Owns the request struct and the Slurm lists inside it
struct AssocMgrRequest {
    ptr: *mut assoc_mgr_info_request_msg_t,
}

impl AssocMgrRequest {
    /// Asks for the QOS records, and for the given users, whose per-user sections are all
    /// the QOS records then carry
    fn new(users: Vec<String>) -> Self {
        let mut req: assoc_mgr_info_request_msg_t = unsafe { std::mem::zeroed() };
        req.flags = ASSOC_MGR_INFO_FLAG_QOS | ASSOC_MGR_INFO_FLAG_USERS;
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

/// What one user, one account, or a whole QOS has running, as the controller counts it
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub jobs: u32,
    pub submitted_jobs: u32,
    /// Keyed by TRES name as the controller names it, e.g. "cpu", "node", "gres/gpu"
    pub tres: HashMap<String, u64>,
}

impl Usage {
    /// A TRES count by Slurm's name for it. Typed GRES are separate entries, so asking for
    /// "gres/gpu" gives the total rather than double counting per-model counts alongside it.
    pub fn tres_count(&self, name: &str) -> u32 {
        self.tres
            .get(name)
            .copied()
            .unwrap_or(0)
            .try_into()
            .unwrap_or(u32::MAX)
    }

    pub fn cores(&self) -> u32 {
        self.tres_count("cpu")
    }

    pub fn nodes(&self) -> u32 {
        self.tres_count("node")
    }

    pub fn gpus(&self) -> u32 {
        self.tres_count("gres/gpu")
    }
}

/// A user as the controller knows them
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub name: String,
    pub uid: u32,
    /// The account a job lands in when it does not name one
    pub default_account: Option<String>,
}

/// A QOS's limits. An absent entry is no limit; a zero is a limit of zero, which permits
/// nothing, so the two are kept apart.
#[derive(Debug, Clone, Default)]
pub struct QosLimits {
    pub max_jobs_per_user: Option<u32>,
    pub max_submit_jobs_per_user: Option<u32>,
    /// MaxTRESPU, keyed by TRES name
    pub max_tres_per_user: HashMap<String, u64>,
    /// GrpTRES, the limit over everyone in the QOS
    pub group_tres: HashMap<String, u64>,
}

/// One QOS, with its limits and the usage counted against them
#[derive(Debug, Clone)]
pub struct QosUsage {
    pub name: String,
    pub limits: QosLimits,
    /// Usage by everyone in the QOS, which is what GrpTRES limits are measured against
    pub group: Usage,
    /// Usage per user, which is what MaxTRESPU and MaxJobsPU are measured against
    pub per_user: HashMap<u32, Usage>,
    /// Usage per account, keyed by account name
    pub per_account: HashMap<String, Usage>,
}

impl QosUsage {
    /// What `uid` has running against this QOS; absent means nothing at all
    pub fn user(&self, uid: u32) -> Usage {
        self.per_user.get(&uid).cloned().unwrap_or_default()
    }

    /// What `account` has running against this QOS; absent means nothing at all
    pub fn account(&self, account: &str) -> Usage {
        self.per_account.get(account).cloned().unwrap_or_default()
    }
}

/// Reads a TRES limit array, which is indexed like the counters. Slurm marks an unset limit
/// INFINITE64 rather than leaving it out, so those drop and genuine zeroes stay.
unsafe fn tres_limit_map(limits: *mut u64, names: &[String]) -> HashMap<String, u64> {
    if limits.is_null() || names.is_empty() {
        return HashMap::new();
    }

    let limits = unsafe { std::slice::from_raw_parts(limits, names.len()) };
    names
        .iter()
        .zip(limits)
        .filter(|&(_, &limit)| limit < u64::MAX - 1)
        .map(|(name, &limit)| (name.clone(), limit))
        .collect()
}

/// Slurm spells an unset scalar limit INFINITE or NO_VAL
fn unlimited_to_none(limit: u32) -> Option<u32> {
    if limit >= u32::MAX - 1 {
        None
    } else {
        Some(limit)
    }
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

/// What the association manager holds: every QOS with its limits and counters, and the
/// users asked after
pub struct AssocMgrInfo {
    /// By QOS name
    pub qos: HashMap<String, QosUsage>,
    /// By user name
    pub users: HashMap<String, UserRecord>,
}

/// Reads the QOS limits and counters, and the given users, from the controller
pub fn load(users: Vec<String>) -> Result<AssocMgrInfo, String> {
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
            let qos = unsafe { read_qos(rec, &tres_names) };
            (qos.name.clone(), qos)
        })
        .collect();

    let users = unsafe { SlurmIterator::new(msg.user_list) }
        .map(|rec| {
            let rec = rec as *const slurmdb_user_rec_t;
            let user = UserRecord {
                name: unsafe { c_str_to_string((*rec).name) },
                uid: unsafe { (*rec).uid },
                default_account: unsafe { non_empty(c_str_to_string((*rec).default_acct)) },
            };
            (user.name.clone(), user)
        })
        .collect();

    Ok(AssocMgrInfo { qos, users })
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// # Safety
/// `rec` must point to a valid QOS record from an association manager reply.
unsafe fn read_qos(rec: *const slurmdb_qos_rec_t, tres_names: &[String]) -> QosUsage {
    let name = unsafe { c_str_to_string((*rec).name) };
    let limits = QosLimits {
        max_jobs_per_user: unlimited_to_none(unsafe { (*rec).max_jobs_pu }),
        max_submit_jobs_per_user: unlimited_to_none(unsafe { (*rec).max_submit_jobs_pu }),
        max_tres_per_user: unsafe { tres_limit_map((*rec).max_tres_pu_ctld, tres_names) },
        group_tres: unsafe { tres_limit_map((*rec).grp_tres_ctld, tres_names) },
    };
    let usage = unsafe { (*rec).usage };

    if usage.is_null() {
        return QosUsage {
            name,
            limits,
            group: Usage::default(),
            per_user: HashMap::new(),
            per_account: HashMap::new(),
        };
    }

    let group = Usage {
        jobs: unsafe { (*usage).grp_used_jobs },
        submitted_jobs: unsafe { (*usage).grp_used_submit_jobs },
        tres: unsafe { tres_map((*usage).grp_used_tres, tres_names) },
    };

    let per_user = unsafe { SlurmIterator::new((*usage).user_limit_list) }
        .map(|entry| {
            let limits = entry as *const slurmdb_used_limits_t;
            (unsafe { (*limits).uid }, unsafe {
                read_used_limits(limits, tres_names)
            })
        })
        .collect();

    let per_account = unsafe { SlurmIterator::new((*usage).acct_limit_list) }
        .map(|entry| {
            let limits = entry as *const slurmdb_used_limits_t;
            (unsafe { c_str_to_string((*limits).acct) }, unsafe {
                read_used_limits(limits, tres_names)
            })
        })
        .collect();

    QosUsage {
        name,
        limits,
        group,
        per_user,
        per_account,
    }
}

/// # Safety
/// `limits` must point to a valid used-limits record from an association manager reply.
unsafe fn read_used_limits(limits: *const slurmdb_used_limits_t, tres_names: &[String]) -> Usage {
    Usage {
        jobs: unsafe { (*limits).jobs },
        submitted_jobs: unsafe { (*limits).submit_jobs },
        tres: unsafe { tres_map((*limits).tres, tres_names) },
    }
}
