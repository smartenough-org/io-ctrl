#![no_std]
#![no_main]
// TODO: Temporarily
#![allow(unused_imports)]

// For static_cell
#![feature(type_alias_impl_trait)]
// Embassy
#![feature(impl_trait_in_assoc_type)]

/*
TODO: Those two (USB, Intercom) are not A#
pub mod usb_comm;
pub mod intercom;
pub mod status;
*/
pub mod components;
pub mod boards;
pub mod app;
