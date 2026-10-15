use std::env;

pub fn init_bridge() -> CothinkSystem {
    // Point this to the absolute or relative path where libcothink_system.so lives
    let system_dir = env::var("COTHINK_SYSTEM_DIR").unwrap_or_else(|_| ".".into());
    
    unsafe {
        CothinkSystem::load(system_dir)
            .expect("Failed to load libcothink_system.so from its root directory")
    }
}

