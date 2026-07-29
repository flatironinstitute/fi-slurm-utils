use bitflags::bitflags;
// bitflags has no dependencies of its own, and is already required as a dependency by bindgen and
// crossterm. As a result, it's not recommended for dependency pruning
use fi_slurm_sys;

bitflags! {
    /// The flags Slurm packs above a node's base state
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NodeStateFlags: u32 {
        const RES = fi_slurm_sys::NODE_STATE_RES;
        const UNDRAIN = fi_slurm_sys::NODE_STATE_UNDRAIN;
        const CLOUD = fi_slurm_sys::NODE_STATE_CLOUD;
        const RESUME = fi_slurm_sys::NODE_RESUME;
        const DRAIN = fi_slurm_sys::NODE_STATE_DRAIN;
        const COMPLETING = fi_slurm_sys::NODE_STATE_COMPLETING;
        const NO_RESPOND = fi_slurm_sys::NODE_STATE_NO_RESPOND;
        const POWERED_DOWN = fi_slurm_sys::NODE_STATE_POWERED_DOWN;
        const FAIL = fi_slurm_sys::NODE_STATE_FAIL;
        const POWERING_UP = fi_slurm_sys::NODE_STATE_POWERING_UP;
        const MAINT = fi_slurm_sys::NODE_STATE_MAINT;
        const REBOOT_REQUESTED = fi_slurm_sys::NODE_STATE_REBOOT_REQUESTED;
        const REBOOT_CANCEL = fi_slurm_sys::NODE_STATE_REBOOT_CANCEL;
        const POWERING_DOWN = fi_slurm_sys::NODE_STATE_POWERING_DOWN;
        const DYNAMIC_FUTURE = fi_slurm_sys::NODE_STATE_DYNAMIC_FUTURE;
        const REBOOT_ISSUED = fi_slurm_sys::NODE_STATE_REBOOT_ISSUED;
        const PLANNED = fi_slurm_sys::NODE_STATE_PLANNED;
        const INVALID_REG = fi_slurm_sys::NODE_STATE_INVALID_REG;
        const POWER_DOWN = fi_slurm_sys::NODE_STATE_POWER_DOWN;
        const POWER_UP = fi_slurm_sys::NODE_STATE_POWER_UP;
        const POWER_DRAIN = fi_slurm_sys::NODE_STATE_POWER_DRAIN;
        const DYNAMIC_NORM = fi_slurm_sys::NODE_STATE_DYNAMIC_NORM;
        const BLOCKED = fi_slurm_sys::NODE_STATE_BLOCKED;
    }
}

bitflags! {
    /// The flags Slurm packs above a job's base state. A job carrying any of these is still
    /// in its base state: COMPLETING and CONFIGURING jobs, for instance, hold their
    /// allocation and count against limits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct JobStateFlags: u32 {
        const LAUNCH_FAILED = fi_slurm_sys::JOB_LAUNCH_FAILED;
        const REQUEUE = fi_slurm_sys::JOB_REQUEUE;
        const REQUEUE_HOLD = fi_slurm_sys::JOB_REQUEUE_HOLD;
        const SPECIAL_EXIT = fi_slurm_sys::JOB_SPECIAL_EXIT;
        const RESIZING = fi_slurm_sys::JOB_RESIZING;
        const CONFIGURING = fi_slurm_sys::JOB_CONFIGURING;
        const COMPLETING = fi_slurm_sys::JOB_COMPLETING;
        const STOPPED = fi_slurm_sys::JOB_STOPPED;
        const RECONFIG_FAIL = fi_slurm_sys::JOB_RECONFIG_FAIL;
        const POWER_UP_NODE = fi_slurm_sys::JOB_POWER_UP_NODE;
        const REVOKED = fi_slurm_sys::JOB_REVOKED;
        const REQUEUE_FED = fi_slurm_sys::JOB_REQUEUE_FED;
        const RESV_DEL_HOLD = fi_slurm_sys::JOB_RESV_DEL_HOLD;
        const SIGNALING = fi_slurm_sys::JOB_SIGNALING;
        const STAGE_OUT = fi_slurm_sys::JOB_STAGE_OUT;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ShowFlags: u16 {
        const ALL = fi_slurm_sys::SHOW_ALL as u16;
        const DETAIL = fi_slurm_sys::SHOW_DETAIL as u16;
        const MIXED = fi_slurm_sys::SHOW_MIXED as u16;
        const LOCAL = fi_slurm_sys::SHOW_LOCAL as u16;
        const SIBLING = fi_slurm_sys::SHOW_SIBLING as u16;
        const FEDERATION = fi_slurm_sys::SHOW_FEDERATION as u16;
        const FUTURE = fi_slurm_sys::SHOW_FUTURE as u16;
    }
}
