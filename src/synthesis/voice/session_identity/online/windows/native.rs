use std::{ffi::c_void, mem::size_of, ptr};

use super::{Result, VoiceError};

pub(super) const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub(super) const PROCESS_VM_READ: u32 = 0x10;
pub(super) const SNAP_PROCESS: u32 = 0x2;
pub(super) const SNAP_MODULE: u32 = 0x8;
pub(super) const ERROR_NO_MORE_FILES: u32 = 18;

#[repr(C)]
pub(super) struct ProcessEntry {
    pub size: u32,
    pub usage: u32,
    pub process_id: u32,
    pub default_heap_id: usize,
    pub module_id: u32,
    pub threads: u32,
    pub parent_process_id: u32,
    pub base_priority: i32,
    pub flags: u32,
    pub exe_file: [u16; 260],
}

impl ProcessEntry {
    pub fn new() -> Self {

        let mut entry: Self = unsafe { std::mem::zeroed() };
        entry.size = size_of::<Self>() as u32;
        entry
    }
}

#[repr(C)]
pub(super) struct ModuleEntry {
    pub size: u32,
    pub module_id: u32,
    pub process_id: u32,
    pub global_usage: u32,
    pub process_usage: u32,
    pub base_address: *mut u8,
    pub image_size: u32,
    pub module: *mut c_void,
    pub module_name: [u16; 256],
    pub exe_path: [u16; 260],
}

impl ModuleEntry {
    pub fn new() -> Self {

        let mut entry: Self = unsafe { std::mem::zeroed() };
        entry.size = size_of::<Self>() as u32;
        entry
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut c_void;
    pub fn CloseHandle(handle: *mut c_void) -> i32;
    pub fn GetCurrentProcess() -> *mut c_void;
    pub fn GetLastError() -> u32;
    pub fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut c_void;
    pub fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
    pub fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
    pub fn Module32FirstW(snapshot: *mut c_void, entry: *mut ModuleEntry) -> i32;
    pub fn QueryFullProcessImageNameW(
        process: *mut c_void,
        flags: u32,
        path: *mut u16,
        length: *mut u32,
    ) -> i32;
    pub fn ReadProcessMemory(
        process: *mut c_void,
        address: *const c_void,
        target: *mut c_void,
        size: usize,
        read: *mut usize,
    ) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    pub fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
    pub fn GetTokenInformation(
        token: *mut c_void,
        information_class: u32,
        information: *mut c_void,
        length: u32,
        returned_length: *mut u32,
    ) -> i32;
    pub fn EqualSid(first: *const c_void, second: *const c_void) -> i32;
}

pub(super) struct Handle(pub *mut c_void);

impl Handle {
    pub fn process(process_id: u32, read_memory: bool) -> Result<Self> {
        let access =
            PROCESS_QUERY_LIMITED_INFORMATION | if read_memory { PROCESS_VM_READ } else { 0 };

        let raw = unsafe { OpenProcess(access, 0, process_id) };
        if raw.is_null() {
            return Err(api_error("OpenProcess"));
        }
        Ok(Self(raw))
    }

    pub fn snapshot(flags: u32, process_id: u32) -> Result<Self> {
        for attempt in 0..3 {

            let raw = unsafe { CreateToolhelp32Snapshot(flags, process_id) };
            if raw != ptr::without_provenance_mut(usize::MAX) {
                return Ok(Self(raw));
            }

            if flags != SNAP_MODULE || unsafe { GetLastError() } != 24 || attempt == 2 {
                return Err(api_error("CreateToolhelp32Snapshot"));
            }
        }
        unreachable!("the final snapshot failure returns an error")
    }
}

impl Drop for Handle {
    fn drop(&mut self) {

        unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn api_error(operation: &str) -> VoiceError {

    let code = unsafe { GetLastError() };
    VoiceError::new(format!("在线模型上下文导入：{operation} 失败（{code}）。"))
}
