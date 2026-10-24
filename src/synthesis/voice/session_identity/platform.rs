use super::super::{Result, VoiceError};

#[cfg(all(windows, target_arch = "x86_64"))]
pub(super) fn machine_identity() -> Result<String> {
    windows::machine_identity()
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
pub(super) fn machine_identity() -> Result<String> {
    Err(VoiceError::new(
        "Duration session identity is currently implemented for Windows x64 only",
    ))
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows {
    use std::{arch::x86_64::__cpuid, ffi::c_void, mem::size_of, ptr};

    use super::{Result, VoiceError};

    const TOKEN_QUERY: u32 = 8;
    const TOKEN_USER: u32 = 1;
    const LMEM_ZEROINIT: u32 = 0x40;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn LocalAlloc(flags: u32, bytes: usize) -> *mut c_void;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        fn GetTokenInformation(
            token: *mut c_void,
            information_class: u32,
            information: *mut c_void,
            information_length: u32,
            returned_length: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *mut c_void, string: *mut *mut u16) -> i32;
    }

    struct Token(*mut c_void);

    impl Drop for Token {
        fn drop(&mut self) {

            unsafe { CloseHandle(self.0) };
        }
    }

    struct LocalMemory(*mut c_void);

    impl Drop for LocalMemory {
        fn drop(&mut self) {

            unsafe { LocalFree(self.0) };
        }
    }

    pub(super) fn machine_identity() -> Result<String> {
        let mut sid_bytes = current_user_sid_bytes()?;

        let user_hash = xxhash_rust::xxh32::xxh32(&sid_bytes, 6);
        for byte in &mut sid_bytes {

            unsafe { std::ptr::write_volatile(byte, 0) };
        }

        let maximum_leaf = __cpuid(0).eax;
        let signature = if maximum_leaf == 0 {
            0
        } else {

            __cpuid(1).eax
        };
        Ok(format!("{user_hash:08x}{signature:08x}"))
    }

    fn current_user_sid_bytes() -> Result<Vec<u8>> {
        let mut token = ptr::null_mut();

        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(api_error("OpenProcessToken"));
        }
        let token = Token(token);
        let mut byte_count = 0;

        unsafe { GetTokenInformation(token.0, TOKEN_USER, ptr::null_mut(), 0, &mut byte_count) };
        if (byte_count as usize) < size_of::<*mut c_void>() {
            return Err(api_error("GetTokenInformation size"));
        }

        let memory = unsafe { LocalAlloc(LMEM_ZEROINIT, byte_count as usize) };
        if memory.is_null() {
            return Err(api_error("LocalAlloc for TokenUser"));
        }
        let memory = LocalMemory(memory);

        if unsafe {
            GetTokenInformation(token.0, TOKEN_USER, memory.0, byte_count, &mut byte_count)
        } == 0
        {
            return Err(api_error("GetTokenInformation TokenUser"));
        }

        let sid = unsafe { *memory.0.cast::<*mut c_void>() };
        let mut wide = ptr::null_mut();

        if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 {
            return Err(api_error("ConvertSidToStringSidW"));
        }
        let _wide_memory = LocalMemory(wide.cast());
        if wide.is_null() {
            return Err(VoiceError::new("ConvertSidToStringSidW returned no string"));
        }
        let mut bytes = Vec::new();
        let mut cursor = wide;
        loop {

            let character = unsafe { *cursor };
            if character == 0 {
                break;
            }
            bytes.extend_from_slice(&character.to_le_bytes());

            cursor = unsafe { cursor.add(1) };
        }
        if bytes.is_empty() {
            return Err(VoiceError::new("Current process TokenUser SID is empty"));
        }
        Ok(bytes)
    }

    fn api_error(operation: &str) -> VoiceError {

        let code = unsafe { GetLastError() };
        VoiceError::new(format!(
            "Duration session identity: {operation} failed ({code})"
        ))
    }
}
