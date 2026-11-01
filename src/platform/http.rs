//! A small HTTPS client over WinHTTP, for the updater and the wallpaper downloads only.
//!
//! Why WinHTTP: TLS, certificate validation, the system proxy and the Windows trust store come
//! from the OS, and there is no new dependency. What this module adds: only the GitHub URLs
//! `update::split_https` accepts, TLS 1.2 or newer, timeouts on every phase, redirects that never
//! leave HTTPS, and a response that is a plain `Read` so the download code can be tested without
//! Windows.
//!
//! Nothing here may panic (the release build aborts on panic); every call reports an error text.

use std::io::Read;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP,
    WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

use super::wide::{pcw, to_wide};

type Handle = *mut core::ffi::c_void;

const RESOLVE_MS: i32 = 15_000;
const CONNECT_MS: i32 = 15_000;
const SEND_MS: i32 = 30_000;
const RECEIVE_MS: i32 = 30_000;

pub struct Response {
    pub status: u32,
    /// The body length the server announced, if it did.
    pub length: Option<u64>,
    session: Handle,
    connect: Handle,
    request: Handle,
}

// The handles are used from the one worker thread that created them.
unsafe impl Send for Response {}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe {
            for handle in [self.request, self.connect, self.session] {
                if !handle.is_null() {
                    WinHttpCloseHandle(handle);
                }
            }
        }
    }
}

fn last_error() -> String {
    crate::update::describe_win_error(unsafe { GetLastError() })
}

/// Start a GET of `url` (one of the allowed GitHub URLs) and read the status line and headers.
pub fn get(url: &str, accept: &str) -> Result<Response, String> {
    let Some((host, port, path)) = crate::update::split_https(url) else {
        return Err("That address is not one this updater is allowed to contact.".into());
    };
    let agent = to_wide(&format!("AllBlack11/{}", env!("CARGO_PKG_VERSION")));
    let host_w = to_wide(&host);
    let verb = to_wide("GET");
    let path_w = to_wide(&path);
    let headers = to_wide(&format!("Accept: {accept}\r\n"));
    unsafe {
        let mut response = Response {
            status: 0,
            length: None,
            session: WinHttpOpen(
                pcw(&agent),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            ),
            connect: null_mut(),
            request: null_mut(),
        };
        if response.session.is_null() {
            return Err(last_error());
        }
        WinHttpSetTimeouts(
            response.session,
            RESOLVE_MS,
            CONNECT_MS,
            SEND_MS,
            RECEIVE_MS,
        );
        // TLS 1.2 and 1.3 only. If the option is refused the OS default still applies.
        let protocols: u32 =
            WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
        WinHttpSetOption(
            response.session,
            WINHTTP_OPTION_SECURE_PROTOCOLS,
            &protocols as *const u32 as *const _,
            4,
        );
        response.connect = WinHttpConnect(response.session, pcw(&host_w), port, 0);
        if response.connect.is_null() {
            return Err(last_error());
        }
        response.request = WinHttpOpenRequest(
            response.connect,
            pcw(&verb),
            pcw(&path_w),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        );
        if response.request.is_null() {
            return Err(last_error());
        }
        // Follow redirects (GitHub serves release files from another host) but never from
        // HTTPS to HTTP.
        let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
        WinHttpSetOption(
            response.request,
            WINHTTP_OPTION_REDIRECT_POLICY,
            &policy as *const u32 as *const _,
            4,
        );
        if WinHttpSendRequest(
            response.request,
            pcw(&headers),
            u32::MAX, // the header text is null-terminated
            std::ptr::null(),
            0,
            0,
            0,
        ) == 0
        {
            return Err(last_error());
        }
        if WinHttpReceiveResponse(response.request, null_mut()) == 0 {
            return Err(last_error());
        }
        let mut status: u32 = 0;
        let mut size: u32 = 4;
        if WinHttpQueryHeaders(
            response.request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut status as *mut u32 as *mut _,
            &mut size,
            null_mut(),
        ) == 0
        {
            return Err(last_error());
        }
        response.status = status;
        let mut length: u32 = 0;
        let mut size: u32 = 4;
        if WinHttpQueryHeaders(
            response.request,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut length as *mut u32 as *mut _,
            &mut size,
            null_mut(),
        ) != 0
        {
            response.length = Some(u64::from(length));
        }
        Ok(response)
    }
}

impl Read for Response {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min(64 * 1024) as u32;
        let mut got: u32 = 0;
        let ok =
            unsafe { WinHttpReadData(self.request, buf.as_mut_ptr() as *mut _, want, &mut got) };
        if ok == 0 {
            return Err(std::io::Error::other(last_error()));
        }
        Ok(got as usize)
    }
}

/// GET a small text body (a release list). At most `max_bytes` are read; a longer answer is an
/// error rather than a truncated parse.
pub fn get_text(url: &str, max_bytes: usize) -> Result<String, String> {
    let mut response = get(url, "application/vnd.github+json")?;
    if response.status != 200 {
        return Err(crate::update::describe_http_status(response.status));
    }
    let mut body: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        let read = response.read(&mut chunk).map_err(|err| err.to_string())?;
        if read == 0 {
            break;
        }
        if body.len() + read > max_bytes {
            return Err("GitHub's answer was larger than expected, so it was not used.".into());
        }
        if let Some(data) = chunk.get(..read) {
            body.extend_from_slice(data);
        }
    }
    String::from_utf8(body).map_err(|_| "GitHub's answer was not text.".to_string())
}
