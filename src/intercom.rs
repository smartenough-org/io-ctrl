use core::cell::RefCell;
use defmt::unwrap;
use embassy_sync::{
    pipe::Pipe,
    blocking_mutex::raw::NoopRawMutex,
};
use embassy_futures::select::{select, Either};
use embedded_io_async::{Read, Write};
use crate::status::{Message, Status};

// Can't use embedded_io_async traits, as they are &mut self.
pub trait Intercom {
    async fn write<'a>(&self, buf: &'a [u8]);
    async fn read<'a>(&self, buf: &'a mut [u8]) -> usize;
}

pub type InterPipe = Pipe<NoopRawMutex, 20>;

pub struct UartIntercom<Uart: Read + Write>
{
    iface: RefCell<Uart>,
    status: &'static Status,

    in_pipe: InterPipe,
    out_pipe: InterPipe,
}

impl<Uart: Read + Write> UartIntercom<Uart>
{
    pub fn new(iface: Uart, status: &'static Status) -> Self {
        Self {
            iface: RefCell::new(iface),
            status,
            in_pipe: Pipe::new(),
            out_pipe: Pipe::new(),
        }
    }

    pub async fn tx_loop(&self) {
        let mut write_buf = [0u8; 128];
        let mut read_buf = [0u8; 128];
        let mut iface = self.iface.borrow_mut();
        self.status.set_state(Message::Attention, 1).await;
        loop {
            // Read pipe or device
            let pipe_reader = self.in_pipe.read(&mut write_buf);
            let iface_reader = iface.read(&mut read_buf);

            match select(pipe_reader, iface_reader).await {
                Either::First(bytes) => {
                    /* Got bytes from pipe, pass to interface */
                    iface.write_all(&write_buf[..bytes]).await.ok();
                },
                Either::Second(bytes) => {
                    /* Got bytes from inteface, pass to pipe */
                    let bytes = unwrap!(bytes.ok()); /* FIXME? */
                    self.out_pipe.write_all(&read_buf[..bytes]).await;
                },
            }
            self.status.set_state(Message::Transfer, 1).await;
            // Timer::after(Duration::from_secs(1)).await;
        }
    }
}

impl<Uart: Read + Write> Intercom for UartIntercom<Uart> {
    async fn read(&self, buf: &mut [u8]) -> usize {
        self.out_pipe.read(buf).await
    }

    async fn write<'a>(&self, buf: &'a [u8]) {
        self.in_pipe.write_all(buf).await;
    }
}
