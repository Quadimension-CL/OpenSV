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
            // SAFETY: Token is constructed only from a successful OpenProcessToken.
            unsafe { CloseHandle(self.0) };
        }
    }

    struct LocalMemory(*mut c_void);

    impl Drop for LocalMemory {
        fn drop(&mut self) {
            // SAFETY: Both allocations used here are owned LocalAlloc allocations.
            unsafe { LocalFree(self.0) };
        }
    }

    pub(super) fn machine_identity() -> Result<String> {
        let mut sid_bytes = current_user_sid_bytes()?;
        // Hash UTF-16LE without its NUL terminator.
        let user_hash = xxhash_rust::xxh32::xxh32(&sid_bytes, 6);
        for byte in &mut sid_bytes {
            // SAFETY: The byte belongs to this mutable, owned SID buffer.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }

        // CPUID is supported by all x86_64 processors. Leaf 0 specifies
        // whether the required leaf 1 request is available.
        let maximum_leaf = __cpuid(0).eax;
        let signature = if maximum_leaf == 0 {
            0
        } else {
            // Leaf 0 reported support for leaf 1.
            __cpuid(1).eax
        };
        Ok(format!("{user_hash:08x}{signature:08x}"))
    }

    fn current_user_sid_bytes() -> Result<Vec<u8>> {
        let mut token = ptr::null_mut();
        // SAFETY: The pseudo process handle remains valid, and token is writable.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(api_error("OpenProcessToken"));
        }
        let token = Token(token);
        let mut byte_count = 0;
        // SAFETY: A null output buffer with length zero requests the required size.
        unsafe { GetTokenInformation(token.0, TOKEN_USER, ptr::null_mut(), 0, &mut byte_count) };
        if (byte_count as usize) < size_of::<*mut c_void>() {
            return Err(api_error("GetTokenInformation size"));
        }
        // SAFETY: The byte count was supplied by the operating system.
        let memory = unsafe { LocalAlloc(LMEM_ZEROINIT, byte_count as usize) };
        if memory.is_null() {
            return Err(api_error("LocalAlloc for TokenUser"));
        }
        let memory = LocalMemory(memory);
        // SAFETY: memory owns byte_count writable, suitably aligned bytes.
        if unsafe {
            GetTokenInformation(token.0, TOKEN_USER, memory.0, byte_count, &mut byte_count)
        } == 0
        {
            return Err(api_error("GetTokenInformation TokenUser"));
        }
        // SAFETY: A successful TokenUser result starts with SID_AND_ATTRIBUTES,
        // whose first member is a SID pointer valid while memory is alive.
        let sid = unsafe { *memory.0.cast::<*mut c_void>() };
        let mut wide = ptr::null_mut();
        // SAFETY: sid comes from TokenUser; wide receives an owned UTF-16 string.
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
            // SAFETY: ConvertSidToStringSidW guarantees a NUL-terminated string.
            // _wide_memory keeps that allocation alive for this entire loop.
            let character = unsafe { *cursor };
            if character == 0 {
                break;
            }
            bytes.extend_from_slice(&character.to_le_bytes());
            // SAFETY: A non-NUL character has a following code unit in the string.
            cursor = unsafe { cursor.add(1) };
        }
        if bytes.is_empty() {
            return Err(VoiceError::new("Current process TokenUser SID is empty"));
        }
        Ok(bytes)
    }

    fn api_error(operation: &str) -> VoiceError {
        // No identity content enters error messages or persistent diagnostics.
        let code = unsafe { GetLastError() };
        VoiceError::new(format!(
            "Duration session identity: {operation} failed ({code})"
        ))
    }
}
