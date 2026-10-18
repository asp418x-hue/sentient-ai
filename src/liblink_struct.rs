use libloading::Library;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeFov {
    pub active_workers: usize,
    pub temp_celsius: f64,
    pub cpu_freq_mhz: u64,
    pub recent_success: usize,
    pub recent_failure: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeThermalSample {
    pub zone_id: u32,
    pub temp_celsius: f64,
    pub cpu_freq_mhz: u64,
    pub is_throttling: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeAnomalyResult {
    pub frame_id: u64,
    pub score: f64,
    pub threshold_crossed: bool,
}

type InitFn = unsafe extern "C" fn() -> i32;
type GetFovFn = unsafe extern "C" fn(u64, *mut NativeFov) -> i32;
type RecordEventFn = unsafe extern "C" fn(u64, usize, *const c_char, bool, *const c_char) -> i32;
type ThermalSampleFn = unsafe extern "C" fn(u32, f64, f64, *mut NativeThermalSample) -> i32;
type ClassifyFn = unsafe extern "C" fn(u64, *mut NativeAnomalyResult) -> i32;
type DispatchFn =
    unsafe extern "C" fn(u64, *const usize, usize, *const c_char, usize, f64) -> *mut c_char;
type ExecutePayloadFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type FreeStringFn = unsafe extern "C" fn(*mut c_char);

pub struct CothinkSystem {
    _lib: Library,
    init: InitFn,
    get_fov: GetFovFn,
    record_event: RecordEventFn,
    thermal_sample: ThermalSampleFn,
    classify: ClassifyFn,
    dispatch_agents: DispatchFn,
    execute_payload: ExecutePayloadFn,
    free_string: FreeStringFn,
}

impl CothinkSystem {
    /// Loads libcothink_system.so dynamically from the specified root directory path.
    ///
    /// # Safety
    /// The shared library must export the cothink symbols with the ABI declared here.
    pub unsafe fn load<P: AsRef<Path>>(path: P) -> Result<Self, libloading::Error> {
        let lib = unsafe { Library::new(path.as_ref().join("libcothink_system.so"))? };
        let init = unsafe { *lib.get::<InitFn>(b"cothink_init\0")? };
        let get_fov = unsafe { *lib.get::<GetFovFn>(b"cothink_get_fov\0")? };
        let record_event = unsafe { *lib.get::<RecordEventFn>(b"cothink_record_event\0")? };
        let thermal_sample = unsafe { *lib.get::<ThermalSampleFn>(b"cothink_thermal_sample\0")? };
        let classify = unsafe { *lib.get::<ClassifyFn>(b"cothink_classify\0")? };
        let dispatch_agents = unsafe { *lib.get::<DispatchFn>(b"cothink_dispatch_agents\0")? };
        let execute_payload =
            unsafe { *lib.get::<ExecutePayloadFn>(b"cothink_execute_payload\0")? };
        let free_string = unsafe { *lib.get::<FreeStringFn>(b"cothink_free_string\0")? };

        Ok(Self {
            _lib: lib,
            init,
            get_fov,
            record_event,
            thermal_sample,
            classify,
            dispatch_agents,
            execute_payload,
            free_string,
        })
    }

    pub fn initialize(&self) -> Result<(), String> {
        status_result(unsafe { (self.init)() })
    }

    pub fn get_fov(&self, orchestrator_id: u64) -> Result<NativeFov, String> {
        let mut result = NativeFov {
            active_workers: 0,
            temp_celsius: 0.0,
            cpu_freq_mhz: 0,
            recent_success: 0,
            recent_failure: 0,
        };
        status_result(unsafe { (self.get_fov)(orchestrator_id, &mut result) })?;
        Ok(result)
    }

    pub fn record_event(
        &self,
        orchestrator_id: u64,
        subagent_id: usize,
        event: &str,
        success: bool,
        detail: &str,
    ) -> Result<(), String> {
        let event = CString::new(event).map_err(|error| error.to_string())?;
        let detail = CString::new(detail).map_err(|error| error.to_string())?;
        status_result(unsafe {
            (self.record_event)(
                orchestrator_id,
                subagent_id,
                event.as_ptr(),
                success,
                detail.as_ptr(),
            )
        })
    }

    pub fn thermal_sample(
        &self,
        zone_id: u32,
        simulated_temp: f64,
        critical_temp: f64,
    ) -> Result<NativeThermalSample, String> {
        let mut result = NativeThermalSample {
            zone_id,
            temp_celsius: 0.0,
            cpu_freq_mhz: 0,
            is_throttling: false,
        };
        status_result(unsafe {
            (self.thermal_sample)(zone_id, simulated_temp, critical_temp, &mut result)
        })?;
        Ok(result)
    }

    pub fn classify(&self, frame_id: u64) -> Result<NativeAnomalyResult, String> {
        let mut result = NativeAnomalyResult {
            frame_id,
            score: 0.0,
            threshold_crossed: false,
        };
        status_result(unsafe { (self.classify)(frame_id, &mut result) })?;
        Ok(result)
    }

    pub fn dispatch_agents(
        &self,
        orchestrator_id: u64,
        task_ids: &[usize],
        context: &str,
        max_concurrent: usize,
        time_dilation_factor: f64,
    ) -> Result<serde_json::Value, String> {
        if task_ids.len() > 256 {
            return Err("a dispatch may contain at most 256 task IDs".to_string());
        }
        let context = CString::new(context).map_err(|error| error.to_string())?;
        let result = unsafe {
            (self.dispatch_agents)(
                orchestrator_id,
                task_ids.as_ptr(),
                task_ids.len(),
                context.as_ptr(),
                max_concurrent,
                time_dilation_factor,
            )
        };
        if result.is_null() {
            return Err("native dispatch returned a null response".to_string());
        }

        let output = unsafe { CStr::from_ptr(result) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.free_string)(result) };

        let response: serde_json::Value = serde_json::from_str(&output)
            .map_err(|error| format!("native dispatch returned invalid JSON: {error}"))?;
        if let Some(error) = response.get("error").and_then(serde_json::Value::as_str) {
            return Err(error.to_string());
        }
        Ok(response)
    }

    pub fn execute_payload(&self, payload_json: &str) -> Result<String, String> {
        let payload = CString::new(payload_json).map_err(|error| error.to_string())?;
        let result = unsafe { (self.execute_payload)(payload.as_ptr()) };
        if result.is_null() {
            return Err("native payload function returned a null string".to_string());
        }

        let output = unsafe { CStr::from_ptr(result) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.free_string)(result) };
        Ok(output)
    }
}

fn status_result(status: i32) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!(
            "native cothink function failed with status {status}"
        ))
    }
}
