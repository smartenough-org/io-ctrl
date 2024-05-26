#![no_std]
#![no_main]
// TODO: Temporarily
#![allow(unused_imports)]
// For static_cell
#![feature(type_alias_impl_trait)]

/*
TODO: Those two (USB, Intercom) are not A#
pub mod usb_comm;
pub mod intercom;
pub mod status;
*/
pub mod app;
pub mod boards;
pub mod components;
pub mod io;
