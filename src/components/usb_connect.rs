use defmt::info;
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_stm32::peripherals::USB;
use embassy_stm32::peripherals::{PA11, PA12};
use embassy_stm32::usb::Driver;
use embassy_stm32::{bind_interrupts, peripherals, usb};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel::Channel;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Builder;
use embassy_usb::UsbDevice;
use static_cell::StaticCell;

use super::message::MessageRaw;

struct Disconnected;

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

type MyDriver = Driver<'static, USB>;
type MyUsb = UsbDevice<'static, MyDriver>;
type MyClass = CdcAcmClass<'static, MyDriver>;

/// Number of bytes transmitted over USB at once. Max size of CommPacket
pub const MAX_PACKET_SIZE: usize = 64;

// addr, type, length, 8 bytes
const CAN_MESSAGE_SIZE: usize = 8 + 3;
pub const CAN_PACKET_SIZE: usize = 2 + CAN_MESSAGE_SIZE;

/// Describes generic message serialized for transfer over USB.
#[derive(defmt::Format)]
pub struct CommPacket {
    /// Number of valid data in packet.
    pub count: u8,
    /// Data from packet.
    pub data: [u8; MAX_PACKET_SIZE],
}

impl Default for CommPacket {
    fn default() -> Self {
        Self {
            count: 0,
            data: [0; MAX_PACKET_SIZE],
        }
    }
}

impl CommPacket {
    /// Byte use to start a packet. Always the same.
    const SYNC_BYTE_1: u8 = 0x21; // !
    /// Second synchronization byte that determines a packet type as well.
    /// 2_CAN uses static 8 byte packet length.
    const SYNC_BYTE_2_CAN: u8 = 0x7C; // |
    const _SYNC_BYTE_2_FDCAN: u8 = 0x7D; // }

    pub fn from_slice(data: &[u8]) -> Self {
        assert!(data.len() < 60);
        let mut p = Self {
            count: data.len() as u8,
            data: [0; MAX_PACKET_SIZE],
        };
        p.data[..data.len()].copy_from_slice(&data[..]);
        p
    }

    /// Serialize raw message into CommPacket
    pub fn from_raw_message(raw: &MessageRaw) -> Self {
        let mut buf = Self::default();
        buf.count = 1 + 1 + 1 + 8;
        (buf.data[0], buf.data[1]) = raw.addr_type();
        buf.data[2] = raw.length();
        buf.data[3..3 + raw.length() as usize].copy_from_slice(raw.data_as_slice());
        buf
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data[0..self.count as usize]
    }

    /// Deserialize from a stream.
    pub fn deserialize_from(buf: &[u8]) -> Option<Self> {
        if buf.len() < 3 {
            defmt::warn!("Unable to decode - message to short {:?}", buf);
            return None;
        }

        if buf[0] != Self::SYNC_BYTE_1 {
            defmt::warn!(
                "Unable to decode message - synchronization failed {:?}",
                buf
            );
            return None;
        }

        let length: usize = match buf[1] {
            Self::SYNC_BYTE_2_CAN => CAN_MESSAGE_SIZE,
            Self::_SYNC_BYTE_2_FDCAN => {
                defmt::warn!("Ignoring unhandled FDCAN on USB");
                return None;
            }
            _ => {
                defmt::warn!("Invalid synchronization - skip message {:?}", buf);
                return None;
            }
        };
        if buf.len() - 2 < length {
            defmt::warn!("Unable to decode message - too short {:?}", buf);
            return None;
        }
        Some(Self::from_slice(&buf[2..2 + length]))
    }

    /// Serialize onto a byte stream.
    pub fn serialize_as_can<'a>(&self, buf: &'a mut [u8]) -> &'a [u8] {
        // Message size at this level is constant to keep things simple.
        buf[0] = Self::SYNC_BYTE_1;
        buf[1] = Self::SYNC_BYTE_2_CAN;
        buf[2..CAN_PACKET_SIZE].copy_from_slice(&self.data[0..CAN_MESSAGE_SIZE]);
        &buf[0..CAN_PACKET_SIZE]
    }
}

pub type CommChannel = Channel<ThreadModeRawMutex, CommPacket, 2>;

/// We use Serial interface for simplicity, but send PACKETS of data.
/// Those need 2 bytes for synchronization, length and data.
struct CommProtocol {
    pub usb_up: &'static CommChannel,
    pub usb_down: &'static CommChannel,
}

impl CommProtocol {
    fn new(usb_up: &'static CommChannel, usb_down: &'static CommChannel) -> Self {
        Self { usb_up, usb_down }
    }

    /// Connection spawner / manager.
    async fn connector(&self, class: &mut MyClass) -> ! {
        loop {
            info!("USB: Awaiting connection.");
            class.wait_connection().await;
            info!("USB: Connected");
            let _ = self.forwarder(class).await;
            info!("USB: Disconnected");
        }
    }

    /// Connection handler
    async fn forwarder(&self, class: &mut MyClass) -> Result<(), Disconnected> {
        loop {
            let mut usb_buf = [0; 64];
            let usb_reader = class.read_packet(&mut usb_buf);
            let ic_reader = self.usb_up.receive();

            match select(usb_reader, ic_reader).await {
                Either::First(bytes) => {
                    match bytes {
                        Ok(bytes) => {
                            defmt::info!("USB RX: {} {:?}", bytes, &usb_buf[0..bytes]);
                            if let Some(msg) = CommPacket::deserialize_from(&usb_buf[0..bytes]) {
                                if self.usb_down.len() >= 1 {
                                    defmt::warn!("Non-empty queue (len={}) when sending msg from USB.", self.usb_down.len());
                                }
                                self.usb_down.send(msg).await;
                                continue;
                            }
                        }
                        Err(err) => {
                            defmt::info!("Not ok! {:?}", err);
                            // Disconnected? Or BufferOverflown
                            return Err(Disconnected);
                        }
                    }
                }
                Either::Second(msg) => {
                    defmt::info!("USB TX: {:?}", msg.as_slice());
                    /* If == 64, then zero-length packet later could be required. */
                    // class.write_packet(&ic_buf[0..bytes]).await?;
                    let mut buf: [u8; CAN_PACKET_SIZE] = [0; CAN_PACKET_SIZE];
                    let buf = msg.serialize_as_can(&mut buf);

                    defmt::info!("USB TX RAW: {:#x}", buf);
                    class.write_packet(buf).await?;
                }
            }
        }
    }
}

pub struct UsbConnect {
    usb: MyUsb,
    class: MyClass,
}

bind_interrupts!(struct Irqs {
    USB_LP => usb::InterruptHandler<peripherals::USB>;
});

// USB interface buffers.
static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static STATE: StaticCell<State> = StaticCell::new();

impl UsbConnect {
    pub fn new(usb_peripheral: USB, dp: PA12, dm: PA11) -> Self {
        // TODO: Maybe pull dp down for reenumeration on flash?

        // TODO: Do vbus detection configuration.
        let driver = Driver::new(usb_peripheral, Irqs, dp, dm);

        // Create embassy-usb Config
        let mut config = embassy_usb::Config::new(0xd10d, 0x10de);
        config.manufacturer = Some("bla");
        config.product = Some("SmartEnough Gate");
        config.serial_number = Some("0000001");

        // Required for windows compatibility.
        // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
        config.device_class = 0xEF;
        config.device_sub_class = 0x02;
        config.device_protocol = 0x01;
        config.composite_with_iads = true;

        // Create embassy-usb DeviceBuilder using the driver and config.
        // It needs some buffers for building the descriptors.
        // let device_descriptor = make_static!([0; 256]);
        let config_descriptor = CONFIG_DESCRIPTOR.init([0; 256]);
        let bos_descriptor = BOS_DESCRIPTOR.init([0; 256]);
        let control_buf = CONTROL_BUF.init([0; 64]);
        let state = STATE.init(State::new());

        let mut builder = Builder::new(
            driver,
            config,
            config_descriptor,
            bos_descriptor,
            &mut [], /* msos descriptors */
            control_buf,
        );

        // Create classes on the builder.
        let class = CdcAcmClass::new(&mut builder, state, MAX_PACKET_SIZE as u16);

        // Build the builder.
        let usb = builder.build();

        Self { usb, class }
    }

    pub async fn run(&mut self, usb_up: &'static CommChannel, usb_down: &'static CommChannel) {
        let usb = self.usb.run();
        let protocol = CommProtocol::new(usb_up, usb_down);
        let connector_future = protocol.connector(&mut self.class);

        info!("Started USB");
        join(usb, connector_future).await;
    }
}
