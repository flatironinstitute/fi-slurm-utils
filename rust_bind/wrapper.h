#include <stddef.h>
#include <slurm/slurm.h>

enum bind_node_state_flags {
    EXTERNAL = (1 << 4),
    RES = (1 << 5),
    UNDRAIN = (1 << 6),
    CLOUD = (1 << 7),
    RESUME = (1 << 8),
    DRAIN = (1 << 9),
    COMPLETING = (1 << 10),
    NO_RESPOND = (1 << 11),
    POWERED_DOWN = (1 << 12),
    FAIL = (1 << 13),
    POWERING_UP = (1 << 14),
    MAINT = (1 << 15),
    REBOOT_REQUESTED = (1 << 16),
    REBOOT_CANCEL = (1 << 17),
    POWERING_DOWN = (1 << 18),
    DYNAMIC_FUTURE = (1 << 19),
    REBOOT_ISSUED = (1 << 20),
    PLANNED = (1 << 21),
    INVALID_REG = (1 << 22),
    POWER_DOWN = (1 << 23),
    POWER_UP = (1 << 24),
    POWER_DRAIN = (1 << 25),
    DYNAMIC_NORM = (1 << 26),
    BLOCKED = (1 << 27)
};
