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

/* Should be less than CdcAcm max packet size (64) */
const CHUNK_SIZE: usize = 32;

pub type InterPipe = Pipe<NoopRawMutex, CHUNK_SIZE>;

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
        let mut write_buf = [0u8; CHUNK_SIZE];
        let mut read_buf = [0u8; CHUNK_SIZE];

        // TODO: Clippy doesn't like this, is it a problem though?
        // This can't be as easily fixed as in status.
        let mut iface = self.iface.borrow_mut();
        self.status.set_state(Message::Attention, 1).await;
        loop {
            // Read pipe or device
            // FIXME: Can those futures be dropped?
            let pipe_reader = self.in_pipe.read(&mut write_buf);
            let iface_reader = iface.read(&mut read_buf);

            match select(pipe_reader, iface_reader).await {
                Either::First(bytes) => {
                    // Got bytes from pipe, pass to interface.
                    // "Sender" should put a back-pressure on the USB sender.
                    // TODO: Proper protocol.
                    iface.write_all(&write_buf[..bytes]).await.ok();
                    defmt::info!("Pushing {} bytes into pipe {:?}...", bytes, &write_buf[..bytes]);
                },
                Either::Second(bytes) => {
                    // Got bytes from underlying inteface, pass to a pipe.
                    // This is allowed to DROP bytes if receiver doesn't read
                    // them fast enough.

                    let bytes = unwrap!(bytes.ok()); // FIXME?
                    defmt::info!("USB->IFACE {} bytes", bytes);
                    let free = self.out_pipe.free_capacity();
                    let write_bytes = core::cmp::min(bytes, free);
                    self.out_pipe.write_all(&read_buf[..write_bytes]).await;
                    if write_bytes < bytes {
                        defmt::info!("USB PIPE FULL {}. DROPPING {}/{} bytes",
                                     free, bytes - write_bytes, bytes);
                    }
                },
            }
            self.status.set_state(Message::Transfer, 1).await;
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
