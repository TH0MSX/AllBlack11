//! Accessible names for Settings' native edit boxes (Q01).
//!
//! An edit control carries no label, so a screen reader reads only "edit". Windows has a service
//! for naming controls you do not own the painting of: `IAccPropServices::SetHwndPropStr` stores a
//! property (here the accessible name and description) for a window handle, and the system's
//! accessibility bridge returns it when a client asks. This is the documented way to label a
//! standard control; it needs no change to the control and no executable code in another process.
//!
//! Failure is not an error: if the service is unavailable the boxes simply stay unlabeled, which
//! is how they behaved before. Nothing here can affect styling, recovery or uninstall.
//!
//! The vtable below was checked against `oleacc.idl` in the Windows SDK. Whether Narrator,
//! NVDA and Accessibility Insights then report these names has NOT been verified on Windows.

use std::ffi::c_void;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::UI::Accessibility::CLSID_AccPropServices;

use super::wide::to_wide;

const IID_IACCPROPSERVICES: GUID = GUID::from_u128(0x6e26e776_04f0_495d_80e4_3330352e3169);
const PROPID_ACC_NAME: GUID = GUID::from_u128(0x608d3df8_8128_4aa7_a428_f55e49267291);
const PROPID_ACC_DESCRIPTION: GUID = GUID::from_u128(0x4d48dfe4_bd3f_491f_a648_492d6f20c588);
/// OBJID_CLIENT as the DWORD the interface takes.
const OBJID_CLIENT: u32 = 0xFFFF_FFFC;
const CHILDID_SELF: u32 = 0;

type SetHwndPropStr = unsafe extern "system" fn(
    this: *mut c_void,
    hwnd: HWND,
    id_object: u32,
    id_child: u32,
    id_prop: GUID,
    value: *const u16,
) -> i32;

/// The start of the IAccPropServices vtable, in the order `oleacc.idl` declares it: IUnknown, then
/// SetPropValue, SetPropServer, ClearProps, SetHwndProp, SetHwndPropStr. Later slots are not used.
#[repr(C)]
struct Vtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    set_prop_value: usize,
    set_prop_server: usize,
    clear_props: usize,
    set_hwnd_prop: usize,
    set_hwnd_prop_str: SetHwndPropStr,
}

#[repr(C)]
struct Object {
    vtbl: *const Vtbl,
}

/// Name each edit box. `edits` is in the order of `labels::EDIT_LABELS`. Returns how many names
/// the system accepted (0 when the service is unavailable).
pub fn label_edits(edits: &[HWND]) -> usize {
    let mut accepted = 0;
    unsafe {
        // Settings runs on the app's main thread. S_FALSE (already initialized) and
        // RPC_E_CHANGED_MODE (initialized another way) are both fine for this call.
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut service: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_AccPropServices,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IACCPROPSERVICES,
            &mut service,
        );
        if hr < 0 || service.is_null() {
            return 0;
        }
        let object = service as *mut Object;
        let vtbl = &*(*object).vtbl;
        for (edit, label) in edits.iter().zip(crate::EDIT_LABELS.iter()) {
            if edit.is_null() {
                continue;
            }
            let name = to_wide(label.name);
            let description = to_wide(label.description);
            let named = (vtbl.set_hwnd_prop_str)(
                service,
                *edit,
                OBJID_CLIENT,
                CHILDID_SELF,
                PROPID_ACC_NAME,
                name.as_ptr(),
            );
            let described = (vtbl.set_hwnd_prop_str)(
                service,
                *edit,
                OBJID_CLIENT,
                CHILDID_SELF,
                PROPID_ACC_DESCRIPTION,
                description.as_ptr(),
            );
            if named >= 0 && described >= 0 {
                accepted += 1;
            }
        }
        (vtbl.release)(service);
    }
    accepted
}
