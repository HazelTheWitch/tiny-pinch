pub mod bevy_utils;

use std::mem::transmute;

use once_cell::unsync::Lazy;
use retour::Function;
use windows::{core::s, Win32::{Foundation::HMODULE, System::LibraryLoader::GetModuleHandleA}};

thread_local! {
    pub static TINY_GLADE: Lazy<HMODULE> = Lazy::new(|| unsafe {
        GetModuleHandleA(s!("tiny-glade.exe")).expect("could not get Tiny Glade module handle")
    });
}

pub unsafe fn transmute_raw(offset: isize) -> *const () {
    TINY_GLADE.with(|tiny_glade| { transmute(tiny_glade.0.wrapping_byte_offset(offset)) })
}

pub unsafe fn transmute_functon<F: Function>(offset: isize) -> F {
    F::from_ptr(transmute_raw(offset))
}

pub const fn exe_space(offset: isize) -> isize {
    offset - 0x140000000
}
