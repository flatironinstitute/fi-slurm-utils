use std::env;
use std::fs;
use std::sync::OnceLock;

static SITE_FN: &str = "site.conf";

// Static global storage that will be initialized on first access
static CLUSTER: OnceLock<Option<String>> = OnceLock::new();

/// Returns the cluster configuration from site.conf
/// The file is read only on first access and its contents are cached
pub fn cluster() -> &'static Option<String> {
    CLUSTER.get_or_init(|| {
        // Try to read from site.conf in the binary's directory
        if let Ok(exe_path) = env::current_exe()
            && let Some(exe_dir) = exe_path.parent()
        {
            let conf_path = exe_dir.join(SITE_FN);
            if let Ok(content) = fs::read_to_string(&conf_path) {
                return Some(content.trim().to_string());
            }
        }
        None
    })
}
