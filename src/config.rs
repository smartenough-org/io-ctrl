/** Constants configuring the crate */

/* NOTE: This could be generics maybe, but maybe const is good enough. */
// pub const MAX_ACTIONS: usize = 32;

#[cfg(feature = "bus-addr-gate")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-addr-1")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-addr-2")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-addr-3")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-addr-4")]
pub const LOCAL_ADDRESS: u8 = 1;
