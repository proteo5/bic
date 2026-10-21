//! Raw C-ABI exports of the BIC decoder for WebAssembly. Spike only.
//!
//! Protocol: JS allocates input with `bic_alloc`, copies the .bic bytes in,
//! calls `bic_decode`, then reads the RGBA result through the output
//! accessors. Returns 0 on success, or a nonzero error code.

use std::cell::RefCell;

use bic::{decode_with_metadata, DecodeLimits};

struct Output {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

thread_local! {
    static LAST: RefCell<Option<Output>> = const { RefCell::new(None) };
}

#[no_mangle]
pub extern "C" fn bic_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn bic_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// Decodes `len` bytes at `ptr`. The limits protect the browser's memory.
#[no_mangle]
pub unsafe extern "C" fn bic_decode(ptr: *const u8, len: usize, max_alloc_mib: u32) -> i32 {
    let input = std::slice::from_raw_parts(ptr, len);
    let limits = DecodeLimits {
        max_alloc: (max_alloc_mib as usize) << 20,
        ..Default::default()
    };
    match decode_with_metadata(input, limits) {
        Ok(d) => {
            LAST.with(|last| {
                *last.borrow_mut() = Some(Output {
                    width: d.width,
                    height: d.height,
                    rgba: d.rgba,
                })
            });
            0
        }
        Err(_) => 1,
    }
}

#[no_mangle]
pub extern "C" fn bic_width() -> u32 {
    LAST.with(|l| l.borrow().as_ref().map_or(0, |o| o.width))
}

#[no_mangle]
pub extern "C" fn bic_height() -> u32 {
    LAST.with(|l| l.borrow().as_ref().map_or(0, |o| o.height))
}

#[no_mangle]
pub extern "C" fn bic_out_ptr() -> *const u8 {
    LAST.with(|l| l.borrow().as_ref().map_or(std::ptr::null(), |o| o.rgba.as_ptr()))
}

#[no_mangle]
pub extern "C" fn bic_out_len() -> usize {
    LAST.with(|l| l.borrow().as_ref().map_or(0, |o| o.rgba.len()))
}

/// Drops the last decoded image.
#[no_mangle]
pub extern "C" fn bic_release() {
    LAST.with(|l| *l.borrow_mut() = None);
}
