/** Constants configuring the crate */

/* NOTE: This could be generics maybe, but maybe const is good enough. */
// pub const MAX_ACTIONS: usize = 32;

#[cfg(feature = "bus-dev-gate")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-dev-1")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-dev-2")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-dev-3")]
pub const LOCAL_ADDRESS: u8 = 1;
#[cfg(feature = "bus-dev-4")]
pub const LOCAL_ADDRESS: u8 = 1;
