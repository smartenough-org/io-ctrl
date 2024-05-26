use crate::io::events::GroupedOutputs;
use crate::io::pcf8575::Pcf8575;
use core::cell::RefCell;
use embassy_time::{Duration, Timer};
use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embedded_hal_async::i2c::I2c;

/// Read inputs (switches) and generate events.
pub struct ExpanderOutputs<BUS: I2c> {
    /// shared i2c bus
    expander: Mutex<NoopRawMutex, Pcf8575<BUS>>,

    state: u16,
}

impl<BUS: I2c> ExpanderOutputs<BUS> {
    pub fn new(expander: Pcf8575<BUS>) -> Self {
        Self {
            expander: Mutex::new(expander),
            state: 0xffff,
        }
    }

    pub async fn reset(&mut self) -> Result<(), ()> {
        self.state = 0xffff;
        self.expander.lock().await.write(self.state).await
    }

    pub async fn set(&mut self, idx: u8, high: bool) -> Result<(), ()> {
        let mask = 1 << idx;
        if mask == 0 {
            defmt::error!("Unable to find IO idx on given outputs");
            return Err(());
        }

        if high {
            self.state |= mask;
        } else {
            self.state &= !mask;
        }

        self.expander.lock().await.write(self.state).await
    }
}

impl<BUS: I2c> GroupedOutputs for ExpanderOutputs<BUS> {
    async fn set_high(&mut self, idx: u8) -> Result<(), ()> {
        self.set(idx, true).await
    }

    async fn set_low(&mut self, idx: u8) -> Result<(), ()> {
        self.set(idx, false).await
    }
}
