use std::env;
use crate::liblink_struct::CothinkSystem;

pub fn init_bridge() -> Result<CothinkSystem, Box<dyn std::error::Error>> {
    // Point this to the absolute or relative path where libcothink_system.so lives
    let system_dir = env::var("COTHINK_SYSTEM_DIR").unwrap_or_else(|_| ".".into());

    Ok(unsafe { CothinkSystem::load(system_dir)? })
}

