/* Constants configuring the crate */

/* NOTE: This could be generics maybe, but maybe const is good enough. */
// pub const MAX_ACTIONS: usize = 32;

pub const MAX_SHUTTERS: usize = 8;

// Max address is 0x3F for compatibility with 11-bit CAN
// TODO: Maybe env!() instead?
#[cfg(feature = "bus-addr-gate")]
pub const LOCAL_ADDRESS: u8 = 0;
#[cfg(feature = "bus-addr-1")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-addr-2")]
pub const LOCAL_ADDRESS: u8 = 2;

// Second floor
#[cfg(feature = "bus-addr-10")]
pub const LOCAL_ADDRESS: u8 = 10;
#[cfg(feature = "bus-addr-11")]
pub const LOCAL_ADDRESS: u8 = 11;

pub const BROADCAST_ADDRESS: u8 = 0x3f;
