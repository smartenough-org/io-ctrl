#![no_std]
#![no_main]

// For static_cell
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]

pub mod usb_comm;
pub mod intercom;
pub mod status;
pub mod actuator;
