//! UI Automation for the custom-drawn Settings window.
//!
//! The Settings window paints its own controls, so Windows cannot see them. This exposes them
//! to screen readers and other UI Automation clients: each control becomes an element with a
//! name, a role, a state and the matching pattern (Invoke, Toggle, SelectionItem, Value).
//!
//! Design, for a reader who has to trust it without having run it:
//!  * **No shared state.** Provider objects hold only the window handle and a stable key. Every
//!    question ("what are the elements?", "what is focused?", "press this") is a window message
//!    to the UI thread (`WM_UIA_QUERY`), answered there, with a one second timeout. The provider
//!    never touches `Ui` from another thread.
//!  * **Nothing here can fail loudly.** No `unwrap`, no unchecked indexing; a bad pointer or a
//!    missing element is an error `HRESULT` the client handles.
//!  * **A kill switch.** Setting the environment variable `ALLBLACK11_NO_UIA=1` makes the window
//!    ignore UI Automation entirely (it then behaves as in 3.6).
//!  * **Hand-written COM**, because the project uses `windows-sys`, which has no interface
//!    wrappers. Interface IDs and method order were checked against Wine's
//!    `uiautomationcore.idl`; the vtable layout follows it exactly.
//!
//! Evidence: cross-compiled. **Never run** with Narrator, Accessibility Insights or any UI
//! Automation client. Treat every behavior described here as unverified until the checklist in
//! `docs/dev/UI-GUIDELINES.md` has been done on Windows.

#![allow(non_snake_case, clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::mem::offset_of;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{
    SysAllocStringLen, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows_sys::Win32::System::Variant::{VARIANT, VT_BOOL, VT_BSTR, VT_I4};
use windows_sys::Win32::UI::Accessibility::{
    UiaClientsAreListening, UiaHostProviderFromHwnd, UiaRaiseAutomationEvent, UiaRect,
    UiaReturnRawElementProvider,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_APP,
};

/// Asked of the UI thread by provider objects.
pub const WM_UIA_QUERY: u32 = WM_APP + 30;
const QUERY_TIMEOUT_MS: u32 = 1000;

const S_OK: HRESULT = 0;
const E_NOINTERFACE: HRESULT = 0x8000_4002_u32 as i32;
const E_POINTER: HRESULT = 0x8000_4003_u32 as i32;
const E_FAIL: HRESULT = 0x8000_4005_u32 as i32;
const UIA_E_ELEMENTNOTAVAILABLE: HRESULT = 0x8004_0201_u32 as i32;
const UIA_E_NOTSUPPORTED: HRESULT = 0x8004_0204_u32 as i32;

// Interface IDs, checked against Wine's uiautomationcore.idl.
const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const IID_SIMPLE: GUID = GUID::from_u128(0xd6dd68d1_86fd_4332_8666_9abedea2d24c);
const IID_FRAGMENT: GUID = GUID::from_u128(0xf7063da8_8359_439c_9297_bbc5299a7d87);
const IID_ROOT: GUID = GUID::from_u128(0x620ce2a5_ab8f_40a9_86cb_de3c75599b58);
const IID_INVOKE: GUID = GUID::from_u128(0x54fcb24b_e18e_47a2_b4d3_eccbe77599a2);
const IID_TOGGLE: GUID = GUID::from_u128(0x56d00bd0_c4f4_433c_a836_1a52a57e0892);
const IID_SELECTION_ITEM: GUID = GUID::from_u128(0x2acad808_b2d4_452d_a407_91ff1ad167b2);
const IID_VALUE: GUID = GUID::from_u128(0xc7935180_6fb3_4201_b174_7df73adbf64a);

// Property, pattern, control-type and event ids (UIAutomationClient.h).
const P_CONTROL_TYPE: i32 = 30003;
const P_LOCALIZED_CONTROL_TYPE: i32 = 30004;
const P_NAME: i32 = 30005;
const P_HAS_KEYBOARD_FOCUS: i32 = 30008;
const P_IS_KEYBOARD_FOCUSABLE: i32 = 30009;
const P_IS_ENABLED: i32 = 30010;
const P_AUTOMATION_ID: i32 = 30011;
const P_HELP_TEXT: i32 = 30013;
const P_IS_CONTROL_ELEMENT: i32 = 30016;
const P_IS_CONTENT_ELEMENT: i32 = 30017;
const P_IS_OFFSCREEN: i32 = 30022;

const PATTERN_INVOKE: i32 = 10000;
const PATTERN_VALUE: i32 = 10002;
const PATTERN_SELECTION_ITEM: i32 = 10010;
const PATTERN_TOGGLE: i32 = 10015;

const CT_BUTTON: i32 = 50000;
const CT_CHECKBOX: i32 = 50002;
const CT_LIST_ITEM: i32 = 50007;
const CT_SLIDER: i32 = 50015;
const CT_TAB_ITEM: i32 = 50019;
const CT_TEXT: i32 = 50020;
const CT_HEADER: i32 = 50034;
const CT_HYPERLINK: i32 = 50005;

const EVENT_FOCUS_CHANGED: i32 = 20005;
const EVENT_LAYOUT_INVALIDATED: i32 = 20008;

const NAV_PARENT: i32 = 0;
const NAV_NEXT: i32 = 1;
const NAV_PREVIOUS: i32 = 2;
const NAV_FIRST_CHILD: i32 = 3;
const NAV_LAST_CHILD: i32 = 4;

const PROVIDER_SERVER_SIDE: i32 = 0x0002;
/// `UiaAppendRuntimeId`: the marker UI Automation expects first in a fragment's runtime id.
const RUNTIME_ID_PREFIX: i32 = 3;

pub const ROOT_KEY: u64 = 0;

/// `GUID` has no `PartialEq` in windows-sys.
fn same(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// Turn the whole thing off. Read once per request, so it can be set before launch.
pub fn disabled() -> bool {
    std::env::var_os("ALLBLACK11_NO_UIA").is_some_and(|value| value != "0")
}

// ---------------------------------------------------------------------------
// What the UI thread tells the provider (plain data, no pointers into Ui)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Text,
    Heading,
    Button,
    /// A hyperlink: Invoke opens it.
    Link,
    Toggle,
    Slider,
    ListItem,
    TabItem,
}

#[derive(Clone, Debug)]
pub struct Info {
    pub key: u64,
    pub name: String,
    pub help: String,
    pub role: Role,
    /// Screen rectangle in physical pixels: left, top, width, height.
    pub rect: [f64; 4],
    pub enabled: bool,
    pub focusable: bool,
    pub focused: bool,
    pub toggled: Option<bool>,
    pub selected: Option<bool>,
    pub value: Option<String>,
    pub automation_id: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    /// Fill `Query::all` with every element in reading order.
    Snapshot,
    /// Press, toggle or select element `key` (the verb is the element's role).
    Act,
    /// Move keyboard focus to element `key`.
    Focus,
}

pub struct Query {
    pub op: Op,
    pub key: u64,
    pub all: Vec<Info>,
    pub ok: bool,
}

impl Query {
    fn new(op: Op, key: u64) -> Self {
        Query {
            op,
            key,
            all: Vec::new(),
            ok: false,
        }
    }
}

/// Ask the UI thread. `None` when the window is gone or did not answer in time.
fn ask(hwnd: HWND, mut query: Query) -> Option<Query> {
    if hwnd.is_null() {
        return None;
    }
    let mut result: usize = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_UIA_QUERY,
            0,
            &mut query as *mut Query as LPARAM,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            QUERY_TIMEOUT_MS,
            &mut result,
        )
    };
    (sent != 0).then_some(query)
}

fn find(all: &[Info], key: u64) -> Option<&Info> {
    all.iter().find(|info| info.key == key)
}

// ---------------------------------------------------------------------------
// The COM object
// ---------------------------------------------------------------------------

type Unknown3 = (
    unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    unsafe extern "system" fn(*mut c_void) -> u32,
    unsafe extern "system" fn(*mut c_void) -> u32,
);

#[repr(C)]
struct SimpleVtbl {
    unknown: Unknown3,
    provider_options: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    get_pattern_provider: unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> HRESULT,
    get_property_value: unsafe extern "system" fn(*mut c_void, i32, *mut VARIANT) -> HRESULT,
    host_provider: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct FragmentVtbl {
    unknown: Unknown3,
    navigate: unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> HRESULT,
    get_runtime_id: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    bounding_rectangle: unsafe extern "system" fn(*mut c_void, *mut UiaRect) -> HRESULT,
    embedded_roots: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    set_focus: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    fragment_root: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct RootVtbl {
    unknown: Unknown3,
    from_point: unsafe extern "system" fn(*mut c_void, f64, f64, *mut *mut c_void) -> HRESULT,
    get_focus: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct InvokeVtbl {
    unknown: Unknown3,
    invoke: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

#[repr(C)]
struct ToggleVtbl {
    unknown: Unknown3,
    toggle: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    state: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
}

#[repr(C)]
struct SelectionItemVtbl {
    unknown: Unknown3,
    select: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    add: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    remove: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    is_selected: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    container: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct ValueVtbl {
    unknown: Unknown3,
    set_value: unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT,
    value: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> HRESULT,
    read_only: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
}

/// One object per element (and one for the window root). Each interface pointer handed out is the
/// address of one of the vtable-pointer fields; `base` turns it back into the object.
#[repr(C)]
struct Element {
    simple: *const SimpleVtbl,
    fragment: *const FragmentVtbl,
    root: *const RootVtbl,
    invoke: *const InvokeVtbl,
    toggle: *const ToggleVtbl,
    select: *const SelectionItemVtbl,
    value: *const ValueVtbl,
    refs: AtomicU32,
    hwnd: isize,
    key: u64,
}

const OFF_SIMPLE: usize = offset_of!(Element, simple);
const OFF_FRAGMENT: usize = offset_of!(Element, fragment);
const OFF_ROOT: usize = offset_of!(Element, root);
const OFF_INVOKE: usize = offset_of!(Element, invoke);
const OFF_TOGGLE: usize = offset_of!(Element, toggle);
const OFF_SELECT: usize = offset_of!(Element, select);
const OFF_VALUE: usize = offset_of!(Element, value);

unsafe fn base<'a>(this: *mut c_void, offset: usize) -> Option<&'a Element> {
    if this.is_null() {
        return None;
    }
    ((this as *mut u8).sub(offset) as *const Element).as_ref()
}

fn new_element(hwnd: HWND, key: u64) -> *mut Element {
    Box::into_raw(Box::new(Element {
        simple: &SIMPLE,
        fragment: &FRAGMENT,
        root: &ROOT,
        invoke: &INVOKE,
        toggle: &TOGGLE,
        select: &SELECT,
        value: &VALUE,
        refs: AtomicU32::new(1),
        hwnd: hwnd as isize,
        key,
    }))
}

/// Interface pointer of kind `offset` for `element`, with a reference taken for the caller.
unsafe fn interface_ptr(element: *mut Element, offset: usize) -> *mut c_void {
    (*element).refs.fetch_add(1, Ordering::AcqRel);
    (element as *mut u8).add(offset) as *mut c_void
}

unsafe extern "system" fn query_interface<const OFF: usize>(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF) else {
        return E_POINTER;
    };
    let element = element as *const Element as *mut Element;
    let wanted = *iid;
    let is_root = element_key(element) == ROOT_KEY;
    let offset = if same(&wanted, &IID_IUNKNOWN) || same(&wanted, &IID_SIMPLE) {
        OFF_SIMPLE
    } else if same(&wanted, &IID_FRAGMENT) {
        OFF_FRAGMENT
    } else if same(&wanted, &IID_ROOT) && is_root {
        OFF_ROOT
    } else if same(&wanted, &IID_INVOKE) {
        OFF_INVOKE
    } else if same(&wanted, &IID_TOGGLE) {
        OFF_TOGGLE
    } else if same(&wanted, &IID_SELECTION_ITEM) {
        OFF_SELECT
    } else if same(&wanted, &IID_VALUE) {
        OFF_VALUE
    } else {
        return E_NOINTERFACE;
    };
    *out = interface_ptr(element, offset);
    S_OK
}

unsafe fn element_key(element: *mut Element) -> u64 {
    (*element).key
}

unsafe extern "system" fn add_ref<const OFF: usize>(this: *mut c_void) -> u32 {
    match base(this, OFF) {
        Some(element) => element.refs.fetch_add(1, Ordering::AcqRel) + 1,
        None => 0,
    }
}

unsafe extern "system" fn release<const OFF: usize>(this: *mut c_void) -> u32 {
    let Some(element) = base(this, OFF) else {
        return 0;
    };
    let remaining = element.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if remaining == 0 {
        drop(Box::from_raw(element as *const Element as *mut Element));
    }
    remaining
}

macro_rules! unknown {
    ($off:expr) => {
        (query_interface::<$off>, add_ref::<$off>, release::<$off>)
    };
}

static SIMPLE: SimpleVtbl = SimpleVtbl {
    unknown: unknown!(OFF_SIMPLE),
    provider_options: simple_options,
    get_pattern_provider: simple_pattern,
    get_property_value: simple_property,
    host_provider: simple_host,
};
static FRAGMENT: FragmentVtbl = FragmentVtbl {
    unknown: unknown!(OFF_FRAGMENT),
    navigate: fragment_navigate,
    get_runtime_id: fragment_runtime_id,
    bounding_rectangle: fragment_rect,
    embedded_roots: fragment_embedded,
    set_focus: fragment_set_focus,
    fragment_root: fragment_root,
};
static ROOT: RootVtbl = RootVtbl {
    unknown: unknown!(OFF_ROOT),
    from_point: root_from_point,
    get_focus: root_get_focus,
};
static INVOKE: InvokeVtbl = InvokeVtbl {
    unknown: unknown!(OFF_INVOKE),
    invoke: invoke_invoke,
};
static TOGGLE: ToggleVtbl = ToggleVtbl {
    unknown: unknown!(OFF_TOGGLE),
    toggle: toggle_toggle,
    state: toggle_state,
};
static SELECT: SelectionItemVtbl = SelectionItemVtbl {
    unknown: unknown!(OFF_SELECT),
    select: select_select,
    add: select_add,
    remove: select_remove,
    is_selected: select_is_selected,
    container: select_container,
};
static VALUE: ValueVtbl = ValueVtbl {
    unknown: unknown!(OFF_VALUE),
    set_value: value_set,
    value: value_get,
    read_only: value_read_only,
};

// --- helpers ---------------------------------------------------------------------------------

fn hwnd_of(element: &Element) -> HWND {
    element.hwnd as HWND
}

fn snapshot(element: &Element) -> Option<Vec<Info>> {
    let answered = ask(hwnd_of(element), Query::new(Op::Snapshot, 0))?;
    Some(answered.all)
}

unsafe fn put_variant_i4(out: *mut VARIANT, value: i32) {
    (*out).Anonymous.Anonymous.vt = VT_I4;
    (*out).Anonymous.Anonymous.Anonymous.lVal = value;
}

unsafe fn put_variant_bool(out: *mut VARIANT, value: bool) {
    (*out).Anonymous.Anonymous.vt = VT_BOOL;
    // VARIANT_TRUE is -1, VARIANT_FALSE is 0.
    (*out).Anonymous.Anonymous.Anonymous.boolVal = if value { -1 } else { 0 };
}

unsafe fn put_variant_text(out: *mut VARIANT, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    (*out).Anonymous.Anonymous.vt = VT_BSTR;
    (*out).Anonymous.Anonymous.Anonymous.bstrVal =
        SysAllocStringLen(wide.as_ptr(), wide.len() as u32);
}

fn control_type(role: Role) -> i32 {
    match role {
        Role::Text => CT_TEXT,
        Role::Heading => CT_HEADER,
        Role::Button => CT_BUTTON,
        Role::Link => CT_HYPERLINK,
        Role::Toggle => CT_CHECKBOX,
        Role::Slider => CT_SLIDER,
        Role::ListItem => CT_LIST_ITEM,
        Role::TabItem => CT_TAB_ITEM,
    }
}

fn role_text(role: Role) -> &'static str {
    match role {
        Role::Text => "text",
        Role::Heading => "heading",
        Role::Button => "button",
        Role::Link => "link",
        Role::Toggle => "switch",
        Role::Slider => "slider",
        Role::ListItem => "list item",
        Role::TabItem => "navigation item",
    }
}

// --- IRawElementProviderSimple -----------------------------------------------------------------

unsafe extern "system" fn simple_options(_this: *mut c_void, out: *mut i32) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = PROVIDER_SERVER_SIDE;
    S_OK
}

unsafe extern "system" fn simple_pattern(
    this: *mut c_void,
    pattern: i32,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_SIMPLE) else {
        return E_POINTER;
    };
    if element.key == ROOT_KEY {
        return S_OK;
    }
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    let Some(info) = find(&all, element.key) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    let target = element as *const Element as *mut Element;
    let offset = match (pattern, info.role) {
        (PATTERN_INVOKE, Role::Button | Role::Link) => Some(OFF_INVOKE),
        (PATTERN_TOGGLE, Role::Toggle) => Some(OFF_TOGGLE),
        (PATTERN_SELECTION_ITEM, Role::ListItem | Role::TabItem) => Some(OFF_SELECT),
        (PATTERN_VALUE, _) if info.value.is_some() => Some(OFF_VALUE),
        _ => None,
    };
    if let Some(offset) = offset {
        *out = interface_ptr(target, offset);
    }
    S_OK
}

unsafe extern "system" fn simple_property(
    this: *mut c_void,
    property: i32,
    out: *mut VARIANT,
) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    // VT_EMPTY by default: "this provider has no opinion".
    std::ptr::write_bytes(out, 0, 1);
    let Some(element) = base(this, OFF_SIMPLE) else {
        return E_POINTER;
    };
    if element.key == ROOT_KEY {
        // The window's own properties come from the host provider.
        return S_OK;
    }
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    let Some(info) = find(&all, element.key) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    match property {
        P_CONTROL_TYPE => put_variant_i4(out, control_type(info.role)),
        P_LOCALIZED_CONTROL_TYPE => put_variant_text(out, role_text(info.role)),
        P_NAME => put_variant_text(out, &info.name),
        P_HELP_TEXT if !info.help.is_empty() => put_variant_text(out, &info.help),
        P_AUTOMATION_ID => put_variant_text(out, &info.automation_id),
        P_IS_ENABLED => put_variant_bool(out, info.enabled),
        P_IS_KEYBOARD_FOCUSABLE => put_variant_bool(out, info.focusable),
        P_HAS_KEYBOARD_FOCUS => put_variant_bool(out, info.focused),
        P_IS_CONTROL_ELEMENT => put_variant_bool(out, true),
        P_IS_CONTENT_ELEMENT => put_variant_bool(out, true),
        P_IS_OFFSCREEN => put_variant_bool(out, false),
        _ => {}
    }
    S_OK
}

unsafe extern "system" fn simple_host(this: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_SIMPLE) else {
        return E_POINTER;
    };
    if element.key != ROOT_KEY {
        return S_OK;
    }
    UiaHostProviderFromHwnd(hwnd_of(element), out)
}

// --- IRawElementProviderFragment ---------------------------------------------------------------

unsafe extern "system" fn fragment_navigate(
    this: *mut c_void,
    direction: i32,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_FRAGMENT) else {
        return E_POINTER;
    };
    let hwnd = hwnd_of(element);
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    let target_key = if element.key == ROOT_KEY {
        match direction {
            NAV_FIRST_CHILD => all.first().map(|info| info.key),
            NAV_LAST_CHILD => all.last().map(|info| info.key),
            _ => None, // parent and siblings of the window come from the host provider
        }
    } else {
        let position = all.iter().position(|info| info.key == element.key);
        match (direction, position) {
            (NAV_PARENT, Some(_)) => Some(ROOT_KEY),
            (NAV_NEXT, Some(i)) => all.get(i + 1).map(|info| info.key),
            (NAV_PREVIOUS, Some(i)) => i
                .checked_sub(1)
                .and_then(|j| all.get(j))
                .map(|info| info.key),
            _ => None,
        }
    };
    if let Some(key) = target_key {
        let created = new_element(hwnd, key);
        *out = interface_ptr(created, OFF_FRAGMENT);
        // `new_element` started at one reference and `interface_ptr` added one: drop ours.
        (*created).refs.fetch_sub(1, Ordering::AcqRel);
    }
    S_OK
}

unsafe extern "system" fn fragment_runtime_id(this: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_FRAGMENT) else {
        return E_POINTER;
    };
    if element.key == ROOT_KEY {
        return S_OK; // the window's id comes from the host provider
    }
    let array = SafeArrayCreateVector(VT_I4, 0, 2);
    if array.is_null() {
        return E_FAIL;
    }
    let ids: [i32; 2] = [RUNTIME_ID_PREFIX, (element.key & 0x7FFF_FFFF) as i32];
    for (index, id) in ids.iter().enumerate() {
        let position = index as i32;
        if SafeArrayPutElement(array, &position, id as *const i32 as *const c_void) < 0 {
            return E_FAIL;
        }
    }
    *out = array as *mut c_void;
    S_OK
}

unsafe extern "system" fn fragment_rect(this: *mut c_void, out: *mut UiaRect) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = UiaRect {
        left: 0.0,
        top: 0.0,
        width: 0.0,
        height: 0.0,
    };
    let Some(element) = base(this, OFF_FRAGMENT) else {
        return E_POINTER;
    };
    if element.key == ROOT_KEY {
        return S_OK; // the host provider reports the window's rectangle
    }
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    let Some(info) = find(&all, element.key) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    *out = UiaRect {
        left: info.rect[0],
        top: info.rect[1],
        width: info.rect[2],
        height: info.rect[3],
    };
    S_OK
}

unsafe extern "system" fn fragment_embedded(_this: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    S_OK
}

unsafe extern "system" fn fragment_set_focus(this: *mut c_void) -> HRESULT {
    let Some(element) = base(this, OFF_FRAGMENT) else {
        return E_POINTER;
    };
    match ask(hwnd_of(element), Query::new(Op::Focus, element.key)) {
        Some(answer) if answer.ok => S_OK,
        _ => UIA_E_ELEMENTNOTAVAILABLE,
    }
}

unsafe extern "system" fn fragment_root(this: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_FRAGMENT) else {
        return E_POINTER;
    };
    let created = new_element(hwnd_of(element), ROOT_KEY);
    *out = interface_ptr(created, OFF_ROOT);
    (*created).refs.fetch_sub(1, Ordering::AcqRel);
    S_OK
}

// --- IRawElementProviderFragmentRoot -----------------------------------------------------------

unsafe extern "system" fn root_from_point(
    this: *mut c_void,
    x: f64,
    y: f64,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_ROOT) else {
        return E_POINTER;
    };
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    // Smallest rectangle containing the point wins, so a button beats the card behind it.
    let hit = all
        .iter()
        .filter(|info| {
            x >= info.rect[0]
                && x < info.rect[0] + info.rect[2]
                && y >= info.rect[1]
                && y < info.rect[1] + info.rect[3]
        })
        .min_by(|a, b| {
            (a.rect[2] * a.rect[3])
                .partial_cmp(&(b.rect[2] * b.rect[3]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    if let Some(info) = hit {
        let created = new_element(hwnd_of(element), info.key);
        *out = interface_ptr(created, OFF_FRAGMENT);
        (*created).refs.fetch_sub(1, Ordering::AcqRel);
    }
    S_OK
}

unsafe extern "system" fn root_get_focus(this: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_ROOT) else {
        return E_POINTER;
    };
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    if let Some(info) = all.iter().find(|info| info.focused) {
        let created = new_element(hwnd_of(element), info.key);
        *out = interface_ptr(created, OFF_FRAGMENT);
        (*created).refs.fetch_sub(1, Ordering::AcqRel);
    }
    S_OK
}

// --- patterns ----------------------------------------------------------------------------------

unsafe fn act(this: *mut c_void, offset: usize) -> HRESULT {
    let Some(element) = base(this, offset) else {
        return E_POINTER;
    };
    match ask(hwnd_of(element), Query::new(Op::Act, element.key)) {
        Some(answer) if answer.ok => S_OK,
        _ => UIA_E_ELEMENTNOTAVAILABLE,
    }
}

unsafe extern "system" fn invoke_invoke(this: *mut c_void) -> HRESULT {
    act(this, OFF_INVOKE)
}

unsafe extern "system" fn toggle_toggle(this: *mut c_void) -> HRESULT {
    act(this, OFF_TOGGLE)
}

unsafe extern "system" fn toggle_state(this: *mut c_void, out: *mut i32) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(element) = base(this, OFF_TOGGLE) else {
        return E_POINTER;
    };
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    match find(&all, element.key).and_then(|info| info.toggled) {
        Some(on) => {
            *out = i32::from(on); // ToggleState_Off = 0, ToggleState_On = 1
            S_OK
        }
        None => UIA_E_NOTSUPPORTED,
    }
}

unsafe extern "system" fn select_select(this: *mut c_void) -> HRESULT {
    act(this, OFF_SELECT)
}

unsafe extern "system" fn select_add(this: *mut c_void) -> HRESULT {
    act(this, OFF_SELECT)
}

unsafe extern "system" fn select_remove(_this: *mut c_void) -> HRESULT {
    // A destination or list item is selected by choosing another one; it cannot be cleared.
    UIA_E_NOTSUPPORTED
}

unsafe extern "system" fn select_is_selected(this: *mut c_void, out: *mut i32) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(element) = base(this, OFF_SELECT) else {
        return E_POINTER;
    };
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    match find(&all, element.key).and_then(|info| info.selected) {
        Some(selected) => {
            *out = i32::from(selected);
            S_OK
        }
        None => UIA_E_NOTSUPPORTED,
    }
}

unsafe extern "system" fn select_container(_this: *mut c_void, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    S_OK
}

unsafe extern "system" fn value_set(_this: *mut c_void, _text: *const u16) -> HRESULT {
    // Values here are read-only: a slider or choice is changed with the keyboard or Invoke.
    UIA_E_NOTSUPPORTED
}

unsafe extern "system" fn value_get(this: *mut c_void, out: *mut *mut u16) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let Some(element) = base(this, OFF_VALUE) else {
        return E_POINTER;
    };
    let Some(all) = snapshot(element) else {
        return UIA_E_ELEMENTNOTAVAILABLE;
    };
    let Some(text) = find(&all, element.key).and_then(|info| info.value.clone()) else {
        return UIA_E_NOTSUPPORTED;
    };
    let wide: Vec<u16> = text.encode_utf16().collect();
    *out = SysAllocStringLen(wide.as_ptr(), wide.len() as u32) as *mut u16;
    S_OK
}

unsafe extern "system" fn value_read_only(_this: *mut c_void, out: *mut i32) -> HRESULT {
    if out.is_null() {
        return E_POINTER;
    }
    *out = -1; // VARIANT_TRUE
    S_OK
}

// ---------------------------------------------------------------------------
// Entry points used by the Settings window
// ---------------------------------------------------------------------------

/// `WM_GETOBJECT` for the window itself. Returns the result to hand back from the window
/// procedure, or `None` to let Windows handle the message.
pub unsafe fn handle_get_object(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    const UIA_ROOT_OBJECT_ID: i32 = -25;
    if disabled() || lparam as i32 != UIA_ROOT_OBJECT_ID {
        return None;
    }
    let root = new_element(hwnd, ROOT_KEY);
    let simple = interface_ptr(root, OFF_SIMPLE);
    let result = UiaReturnRawElementProvider(hwnd, wparam, lparam, simple);
    // `UiaReturnRawElementProvider` takes its own reference; give back ours and the creation one.
    release::<OFF_SIMPLE>(simple);
    release::<OFF_SIMPLE>(simple);
    Some(result)
}

/// The window is being destroyed: let go of every provider UI Automation still holds.
pub unsafe fn disconnect(hwnd: HWND) {
    if !disabled() {
        UiaReturnRawElementProvider(hwnd, 0, 0, std::ptr::null_mut());
    }
}

/// Tell listening clients (a screen reader) that keyboard focus moved to element `key`, or that
/// the layout changed. Cheap when nobody is listening.
pub unsafe fn raise_focus(hwnd: HWND, key: u64) {
    if disabled() || UiaClientsAreListening() == 0 {
        return;
    }
    let element = new_element(hwnd, key);
    let provider = interface_ptr(element, OFF_SIMPLE);
    UiaRaiseAutomationEvent(provider, EVENT_FOCUS_CHANGED);
    release::<OFF_SIMPLE>(provider);
    release::<OFF_SIMPLE>(provider);
}

/// Is any UI Automation client (a screen reader) listening for events right now?
pub fn listening() -> bool {
    !disabled() && unsafe { UiaClientsAreListening() } != 0
}

pub unsafe fn raise_layout_changed(hwnd: HWND) {
    if disabled() || UiaClientsAreListening() == 0 {
        return;
    }
    let element = new_element(hwnd, ROOT_KEY);
    let provider = interface_ptr(element, OFF_SIMPLE);
    UiaRaiseAutomationEvent(provider, EVENT_LAYOUT_INVALIDATED);
    release::<OFF_SIMPLE>(provider);
    release::<OFF_SIMPLE>(provider);
}

/// Client-area rectangle (logical px scaled to physical, relative to the client) to screen.
pub fn to_screen(hwnd: HWND, rect_px: (i32, i32, i32, i32)) -> [f64; 4] {
    let mut origin = POINT {
        x: rect_px.0,
        y: rect_px.1,
    };
    unsafe { ClientToScreen(hwnd, &mut origin) };
    [
        f64::from(origin.x),
        f64::from(origin.y),
        f64::from((rect_px.2 - rect_px.0).max(0)),
        f64::from((rect_px.3 - rect_px.1).max(0)),
    ]
}

#[allow(dead_code)]
fn unused(_: RECT) {}

// ---------------------------------------------------------------------------
// Self-test (QA aid)
// ---------------------------------------------------------------------------

/// Walk the element tree through the COM vtables exactly as a UI Automation client would, from
/// a worker thread (as UI Automation calls arrive), and report what it found. It checks the
/// parts a hand-written COM object can get wrong without a client to notice: interface
/// lookup, reference counts, vtable order, the thread hand-off and the string and array
/// results. It presses nothing.
///
/// Run it by starting AllBlack11 with `ALLBLACK11_UIA_SELFTEST=1`; the result goes to the log
/// and `uia-selftest.txt` in the data folder.
pub fn self_test(hwnd: HWND) -> Vec<String> {
    let handle = hwnd as isize;
    std::thread::spawn(move || unsafe { run_self_test(handle as HWND) })
        .join()
        .unwrap_or_else(|_| vec!["FAIL the self-test thread panicked".to_string()])
}

unsafe fn vtable<T>(interface: *mut c_void) -> &'static T {
    &**(interface as *mut *const T)
}

unsafe fn bstr_text(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    // A BSTR carries its length in the four bytes before the characters.
    let length = *((text as *const u8).sub(4) as *const u32) as usize / 2;
    String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
}

unsafe fn run_self_test(hwnd: HWND) -> Vec<String> {
    let mut report = Vec::new();
    fn check(report: &mut Vec<String>, ok: bool, what: &str) -> bool {
        report.push(format!("{} {what}", if ok { "PASS" } else { "FAIL" }));
        ok
    }
    let root = new_element(hwnd, ROOT_KEY);
    let simple = interface_ptr(root, OFF_SIMPLE);
    // The creation reference plus the one just taken.
    check(
        &mut report,
        (*root).refs.load(Ordering::Acquire) == 2,
        "a new object starts with the references expected",
    );

    let mut fragment: *mut c_void = std::ptr::null_mut();
    let hr = (vtable::<SimpleVtbl>(simple).unknown.0)(simple, &IID_FRAGMENT, &mut fragment);
    check(
        &mut report,
        hr == S_OK && !fragment.is_null(),
        "QueryInterface finds the fragment interface",
    );
    let mut root_iface: *mut c_void = std::ptr::null_mut();
    let hr = (vtable::<SimpleVtbl>(simple).unknown.0)(simple, &IID_ROOT, &mut root_iface);
    check(
        &mut report,
        hr == S_OK && !root_iface.is_null(),
        "the window root exposes the fragment-root interface",
    );
    let mut nothing: *mut c_void = std::ptr::null_mut();
    let bogus = GUID::from_u128(0x12345678_1234_1234_1234_123456789abc);
    let hr = (vtable::<SimpleVtbl>(simple).unknown.0)(simple, &bogus, &mut nothing);
    check(
        &mut report,
        hr == E_NOINTERFACE && nothing.is_null(),
        "an unknown interface is refused",
    );

    let mut options = 0;
    (vtable::<SimpleVtbl>(simple).provider_options)(simple, &mut options);
    check(
        &mut report,
        options == PROVIDER_SERVER_SIDE,
        "the provider says it is server side",
    );

    // First child, then every sibling.
    let mut count = 0usize;
    let mut buttons = 0usize;
    let mut named = 0usize;
    let mut with_rect = 0usize;
    let mut current: *mut c_void = std::ptr::null_mut();
    let hr = (vtable::<FragmentVtbl>(fragment).navigate)(fragment, NAV_FIRST_CHILD, &mut current);
    check(
        &mut report,
        hr == S_OK && !current.is_null(),
        "the root has a first child",
    );
    while !current.is_null() && count < 400 {
        count += 1;
        let mut child_simple: *mut c_void = std::ptr::null_mut();
        (vtable::<FragmentVtbl>(current).unknown.0)(current, &IID_SIMPLE, &mut child_simple);
        if !child_simple.is_null() {
            let mut name: VARIANT = std::mem::zeroed();
            (vtable::<SimpleVtbl>(child_simple).get_property_value)(
                child_simple,
                P_NAME,
                &mut name,
            );
            let text = bstr_text(name.Anonymous.Anonymous.Anonymous.bstrVal);
            if !text.is_empty() {
                named += 1;
            }
            let mut kind: VARIANT = std::mem::zeroed();
            (vtable::<SimpleVtbl>(child_simple).get_property_value)(
                child_simple,
                P_CONTROL_TYPE,
                &mut kind,
            );
            if kind.Anonymous.Anonymous.Anonymous.lVal == CT_BUTTON {
                buttons += 1;
                // A button must offer Invoke.
                let mut pattern: *mut c_void = std::ptr::null_mut();
                (vtable::<SimpleVtbl>(child_simple).get_pattern_provider)(
                    child_simple,
                    PATTERN_INVOKE,
                    &mut pattern,
                );
                if pattern.is_null() {
                    report.push("FAIL a button does not offer the Invoke pattern".to_string());
                } else {
                    (vtable::<InvokeVtbl>(pattern).unknown.2)(pattern);
                }
            }
            (vtable::<SimpleVtbl>(child_simple).unknown.2)(child_simple);
        }
        let mut rect = UiaRect {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
        };
        (vtable::<FragmentVtbl>(current).bounding_rectangle)(current, &mut rect);
        if rect.width > 0.0 && rect.height > 0.0 {
            with_rect += 1;
        }
        let mut runtime_id: *mut c_void = std::ptr::null_mut();
        (vtable::<FragmentVtbl>(current).get_runtime_id)(current, &mut runtime_id);
        if runtime_id.is_null() {
            report.push("FAIL an element has no runtime id".to_string());
        }
        let mut next: *mut c_void = std::ptr::null_mut();
        (vtable::<FragmentVtbl>(current).navigate)(current, NAV_NEXT, &mut next);
        (vtable::<FragmentVtbl>(current).unknown.2)(current);
        current = next;
    }
    report.push(format!(
        "INFO {count} elements, {named} named, {buttons} buttons, {with_rect} with a rectangle"
    ));
    check(
        &mut report,
        count > 5,
        "the window exposes more than a handful of elements",
    );
    check(
        &mut report,
        named * 10 >= count * 9,
        "at least 90 percent of elements have a name",
    );
    check(
        &mut report,
        with_rect * 10 >= count * 8,
        "at least 80 percent of elements have an on-screen rectangle",
    );

    // Focus and hit testing answer without error.
    let mut focus: *mut c_void = std::ptr::null_mut();
    let hr = (vtable::<RootVtbl>(root_iface).get_focus)(root_iface, &mut focus);
    check(&mut report, hr == S_OK, "GetFocus answers");
    if !focus.is_null() {
        (vtable::<FragmentVtbl>(focus).unknown.2)(focus);
    }

    // Release everything we took and check nothing leaked or double-freed.
    (vtable::<FragmentVtbl>(fragment).unknown.2)(fragment);
    (vtable::<RootVtbl>(root_iface).unknown.2)(root_iface);
    let before = (*root).refs.load(Ordering::Acquire);
    check(
        &mut report,
        before == 2,
        "after releasing what the test took, only the original two references remain",
    );
    (vtable::<SimpleVtbl>(simple).unknown.2)(simple);
    (vtable::<SimpleVtbl>(simple).unknown.2)(simple); // frees the object
    report
}
