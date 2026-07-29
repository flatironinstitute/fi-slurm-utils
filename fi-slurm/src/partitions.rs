use crate::states::ShowFlags;
use crate::utils::c_str_to_string;
use fi_slurm_sys::{
    partition_info_msg_t, partition_info_t, slurm_free_partition_info_msg, slurm_load_partitions,
    time_t,
};

/// Slurm spells an unrestricted account or group list this way
const UNRESTRICTED: &str = "ALL";

/// We use this struct to manage the C-allocated memory,
/// automatically dropping it when it goes out of scope
pub struct RawSlurmPartitionInfo {
    ptr: *mut partition_info_msg_t,
}

impl Drop for RawSlurmPartitionInfo {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_free_partition_info_msg(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

impl RawSlurmPartitionInfo {
    /// Loads all partition information from the Slurm controller.
    ///
    /// This is the only function that directly calls the unsafe `slurm_load_partitions`
    /// FFI function. On success, it returns an instance of the safe RAII wrapper.
    pub fn load(update_time: time_t) -> Result<Self, String> {
        let mut part_info_msg_ptr: *mut partition_info_msg_t = std::ptr::null_mut();

        // ALL so that the records don't depend on who is running this, which would give the
        // wrong answer when reporting on another user
        let show_flags = ShowFlags::ALL;

        let return_code = unsafe {
            slurm_load_partitions(update_time, &mut part_info_msg_ptr, show_flags.bits())
        };

        if return_code != 0 || part_info_msg_ptr.is_null() {
            Err("Failed to load partition information from Slurm".to_string())
        } else {
            Ok(Self {
                ptr: part_info_msg_ptr,
            })
        }
    }

    /// Provides safe, read-only access to the partition data as a Rust slice
    pub fn as_slice(&self) -> &[partition_info_t] {
        if self.ptr.is_null() {
            return &[];
        }
        // This is `unsafe` because we are promising the compiler that the pointer
        // and record_count from the C library are valid
        unsafe {
            let msg = &*self.ptr;
            std::slice::from_raw_parts(msg.partition_array, msg.record_count as usize)
        }
    }

    /// Consumes the wrapper to transform the raw C data into safe, owned `Partition` values
    pub fn into_partitions(self) -> Vec<Partition> {
        self.as_slice().iter().map(Partition::from_raw).collect()
    }
}

/// A safe, owned representation of a Slurm partition, holding the fields needed to tell
/// who may submit to it and which limits apply there
#[derive(Debug, Clone)]
pub struct Partition {
    pub name: String,
    /// The partition QOS, whose limits apply to every job in the partition regardless of the
    /// QOS the job requests. `None` where the partition has no QOS of its own.
    pub qos: Option<String>,
    allow_accounts: Option<Vec<String>>,
    deny_accounts: Option<Vec<String>>,
}

impl Partition {
    fn from_raw(raw: &partition_info_t) -> Self {
        // Slurm rejects setting both lists on one partition, so at most one is populated
        let (allow_accounts, deny_accounts) = unsafe {
            (
                account_list(raw.allow_accounts),
                account_list(raw.deny_accounts),
            )
        };

        Self {
            name: unsafe { c_str_to_string(raw.name) },
            qos: unsafe { non_empty(c_str_to_string(raw.qos_char)) },
            allow_accounts,
            deny_accounts,
        }
    }

    /// Whether `account` may submit to this partition. Group restrictions (`AllowGroups`) are
    /// not considered, so a partition open to an account but closed to the user's groups
    /// still counts as allowed.
    pub fn allows_account(&self, account: &str) -> bool {
        if let Some(denied) = &self.deny_accounts {
            return !denied.iter().any(|a| a == account);
        }

        match &self.allow_accounts {
            Some(allowed) => allowed.iter().any(|a| a == account),
            None => true,
        }
    }
}

/// Parses a Slurm comma-separated account list, mapping "unrestricted" onto `None`
unsafe fn account_list(ptr: *const i8) -> Option<Vec<String>> {
    let raw = unsafe { non_empty(c_str_to_string(ptr))? };
    if raw == UNRESTRICTED {
        return None;
    }
    Some(raw.split(',').map(|a| a.to_string()).collect())
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

/// Fetches all partitions from Slurm and returns them as safe, owned Rust values
pub fn get_partitions() -> Result<Vec<Partition>, String> {
    Ok(RawSlurmPartitionInfo::load(0)?.into_partitions())
}
