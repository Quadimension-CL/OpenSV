use std::{ffi::c_void, mem::size_of, ptr};

use zeroize::Zeroizing;

use super::{Result, VoiceError, native::*};

const TOKEN_QUERY: u32 = 8;
const TOKEN_USER: u32 = 1;
const TOKEN_SESSION_ID: u32 = 12;

pub(super) fn same_user_and_session(process: &Handle) -> Result<bool> {
    // SAFETY: The pseudo handle is valid and is borrowed, never closed.
    let current = token(unsafe { GetCurrentProcess() })?;
    let target = token(process.0)?;
    if session_id(&current)? != session_id(&target)? {
        return Ok(false);
    }
    let current_user = user(&current)?;
    let target_user = user(&target)?;
    // SAFETY: TokenUser begins with SID_AND_ATTRIBUTES. Both buffers are
    // aligned, and remain alive while the operating system compares their SIDs.
    let equal = unsafe {
        EqualSid(
            current_user[0] as *const c_void,
            target_user[0] as *const c_void,
        )
    };
    Ok(equal != 0)
}

fn token(process: *mut c_void) -> Result<Handle> {
    let mut raw = ptr::null_mut();
    // SAFETY: process is a live handle and raw is a writable output pointer.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw) } == 0 {
        return Err(api_error("OpenProcessToken"));
    }
    Ok(Handle(raw))
}

fn session_id(token: &Handle) -> Result<u32> {
    let mut session = 0;
    let mut size = 0;
    // SAFETY: TokenSessionId returns a DWORD to this correctly-sized output.
    if unsafe {
        GetTokenInformation(
            token.0,
            TOKEN_SESSION_ID,
            ptr::from_mut(&mut session).cast(),
            size_of::<u32>() as u32,
            &mut size,
        )
    } == 0
    {
        return Err(api_error("GetTokenInformation session"));
    }
    if size != size_of::<u32>() as u32 {
        return Err(VoiceError::new("无法验证所选进程的 Windows 会话。"));
    }
    Ok(session)
}

fn user(token: &Handle) -> Result<Zeroizing<Vec<usize>>> {
    let mut bytes = 0;
    // SAFETY: A null output with zero length asks for the required buffer size.
    unsafe { GetTokenInformation(token.0, TOKEN_USER, ptr::null_mut(), 0, &mut bytes) };
    if !(size_of::<usize>() as u32..=65536).contains(&bytes) {
        return Err(api_error("GetTokenInformation user size"));
    }
    // usize alignment is required because TOKEN_USER contains a native pointer.
    let mut buffer = Zeroizing::new(vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())]);
    let mut written = 0;
    // SAFETY: The owned output holds at least bytes aligned writable bytes.
    if unsafe {
        GetTokenInformation(
            token.0,
            TOKEN_USER,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut written,
        )
    } == 0
    {
        return Err(api_error("GetTokenInformation user"));
    }
    if written > bytes || written < size_of::<usize>() as u32 || buffer[0] == 0 {
        return Err(VoiceError::new("无法验证所选进程的 Windows 用户。"));
    }
    Ok(buffer)
}
