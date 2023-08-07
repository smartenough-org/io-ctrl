use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::Peripheral;
use embassy_stm32::spi::{Instance, SckPin, MosiPin, MisoPin, TxDma, RxDma};
use embassy_stm32::time::Hertz;
use embedded_hal_async::spi::SpiBus;
use core::marker::PhantomData;

use crate::status::{Message, Status};

pub struct Intercom<SPI: SpiBus>
{
    spi: SPI,
    status: &'static Status,
}

impl<SPI: SpiBus> Intercom<SPI>
{
    pub fn new(spi: SPI, status: &'static Status) -> Self {
        Self {
            spi,
            status
        }
    }

    pub async fn rx_loop(&mut self) {
    }

    pub async fn tx_loop(&mut self) {
        let write = [0u8; 128];
        let mut read = [0u8; 128];
        // core::write!(&mut write, "Hello DMA World!\n").unwrap();
        self.status.set_state(Message::Attention, 1).await;
        loop {
            self.spi.transfer(&mut read[0..write.len()], &write).await.ok();
        }
    }
}
