use bitflags::bitflags;
// bitflags has no dependencies of its own, and is already required as a dependency by bindgen and
// crossterm. As a result, it's not recommended for dependency pruning
use fi_slurm_sys;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NodeStateFlags: u32 {
        const NET = fi_slurm_sys::bind_node_state_flags_NET;
        const RES = fi_slurm_sys::bind_node_state_flags_RES;
        const UNDRAIN = fi_slurm_sys::bind_node_state_flags_UNDRAIN;
        const CLOUD = fi_slurm_sys::bind_node_state_flags_CLOUD;
        const RESUME = fi_slurm_sys::bind_node_state_flags_RESUME;
        const DRAIN = fi_slurm_sys::bind_node_state_flags_DRAIN;
        const COMPLETING = fi_slurm_sys::bind_node_state_flags_COMPLETING;
        const NO_RESPOND = fi_slurm_sys::bind_node_state_flags_NO_RESPOND;
        const POWERED_DOWN = fi_slurm_sys::bind_node_state_flags_POWERED_DOWN;
        const FAIL = fi_slurm_sys::bind_node_state_flags_FAIL;
        const POWERING_UP = fi_slurm_sys::bind_node_state_flags_POWERING_UP;
        const MAINT = fi_slurm_sys::bind_node_state_flags_MAINT;
        const REBOOT_REQUESTED = fi_slurm_sys::bind_node_state_flags_REBOOT_REQUESTED;
        const REBOOT_CANCEL = fi_slurm_sys::bind_node_state_flags_REBOOT_CANCEL;
        const POWERING_DOWN = fi_slurm_sys::bind_node_state_flags_POWERING_DOWN;
        const DYNAMIC_FUTURE = fi_slurm_sys::bind_node_state_flags_DYNAMIC_FUTURE;
        const REBOOT_ISSUED = fi_slurm_sys::bind_node_state_flags_REBOOT_ISSUED;
        const PLANNED = fi_slurm_sys::bind_node_state_flags_PLANNED;
        const INVALID_REG = fi_slurm_sys::bind_node_state_flags_INVALID_REG;
        const POWER_DOWN = fi_slurm_sys::bind_node_state_flags_POWER_DOWN;
        const POWER_UP = fi_slurm_sys::bind_node_state_flags_POWER_UP;
        const POWER_DRAIN = fi_slurm_sys::bind_node_state_flags_POWER_DRAIN;
        const DYNAMIC_NORM = fi_slurm_sys::bind_node_state_flags_DYNAMIC_NORM;
        const BLOCKED = fi_slurm_sys::bind_node_state_flags_PLANNED;
    }
}
