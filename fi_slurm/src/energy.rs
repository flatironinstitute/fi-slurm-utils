use chrono::{DateTime, Utc};
use rust_bind::bindings::acct_gather_energy_t;
use crate::utils::time_t_to_datetime;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct AcctGatherEnergy {
    average_watts: u32, // average power consumption of node, in watts
    base_consumed_energy: u64,
    consumed_energy: u64, // joules
    current_watts: u32,
    last_adjustment: u64, // joules
    previous_consumed_energy: u64,
    poll_time: DateTime<Utc>, // when information was last retrieved
    slurmd_start_time: DateTime<Utc>,
}

impl AcctGatherEnergy {
    /// Creates a safe, owned Rust `AcctGatherEnergy` struct from a raw
    /// C-style `acct_gather_energy_t` struct
    ///
    /// # Safety
    ///
    /// The caller must ensure that `raw_energy` is a valid, non-null pointer
    pub fn from_raw_binding(
        raw_energy: &acct_gather_energy_t,
    ) -> Result<Self, String> {

        Ok(AcctGatherEnergy {
            average_watts: raw_energy.ave_watts,
            base_consumed_energy: raw_energy.base_consumed_energy,
            consumed_energy: raw_energy.consumed_energy,
            current_watts: raw_energy.current_watts,
            last_adjustment: raw_energy.last_adjustment, // Assuming direct mapping
            previous_consumed_energy: raw_energy.previous_consumed_energy, // Assuming direct mapping
            poll_time: time_t_to_datetime(raw_energy.poll_time),
            slurmd_start_time: time_t_to_datetime(raw_energy.slurmd_start_time),
        })
    }
}
