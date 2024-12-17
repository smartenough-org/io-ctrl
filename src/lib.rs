#![no_std]
#![no_main]

// Adding/multiplying const expressions
// #![feature(generic_const_exprs)]

/*
TODO: Those two (USB, Intercom) are not A#
pub mod usb_comm;
pub mod intercom;
pub mod status;
*/
pub mod app;
pub mod boards;
pub mod buttonsmash;
pub mod components;
pub mod config;
pub mod io;

pub fn stack_addr() {
    let a: u32 = 0;
    let ap = &a as *const u32;
    let mem_size = 32768;
    let diff = mem_size - (ap as u64).wrapping_sub(0x20000000u64);
    defmt::info!(
        "CUR Stack address is {:#02x}, {:#02x}, {} bytes",
        ap,
        diff,
        diff
    );
}
