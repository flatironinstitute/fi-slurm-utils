use bitflags::bitflags;
// bitflags has no dependencies of its own, and is already required as a dependency by bindgen and
// crossterm. As a result, it's not recommended for dependency pruning
use rust_bind::bindings;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NodeStateFlags: u32 {
        const NET = bindings::bind_node_state_flags_NET;
        const RES = bindings::bind_node_state_flags_RES;
        const UNDRAIN = bindings::bind_node_state_flags_UNDRAIN;
        const CLOUD = bindings::bind_node_state_flags_CLOUD;
        const RESUME = bindings::bind_node_state_flags_RESUME;
        const DRAIN = bindings::bind_node_state_flags_DRAIN;
        const COMPLETING = bindings::bind_node_state_flags_COMPLETING;
        const NO_RESPOND = bindings::bind_node_state_flags_NO_RESPOND;
        const POWERED_DOWN = bindings::bind_node_state_flags_POWERED_DOWN;
        const FAIL = bindings::bind_node_state_flags_FAIL;
        const POWERING_UP = bindings::bind_node_state_flags_POWERING_UP;
        const MAINT = bindings::bind_node_state_flags_MAINT;
        const REBOOT_REQUESTED = bindings::bind_node_state_flags_REBOOT_REQUESTED;
        const REBOOT_CANCEL = bindings::bind_node_state_flags_REBOOT_CANCEL;
        const POWERING_DOWN = bindings::bind_node_state_flags_POWERING_DOWN;
        const DYNAMIC_FUTURE = bindings::bind_node_state_flags_DYNAMIC_FUTURE;
        const REBOOT_ISSUED = bindings::bind_node_state_flags_REBOOT_ISSUED;
        const PLANNED = bindings::bind_node_state_flags_PLANNED;
        const INVALID_REG = bindings::bind_node_state_flags_INVALID_REG;
        const POWER_DOWN = bindings::bind_node_state_flags_POWER_DOWN;
        const POWER_UP = bindings::bind_node_state_flags_POWER_UP;
        const POWER_DRAIN = bindings::bind_node_state_flags_POWER_DRAIN;
        const DYNAMIC_NORM = bindings::bind_node_state_flags_DYNAMIC_NORM;
        const BLOCKED = bindings::bind_node_state_flags_PLANNED;
    }
}
