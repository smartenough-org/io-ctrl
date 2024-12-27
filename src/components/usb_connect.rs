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

pub const MAX_PACKET_SIZE: usize = 64;

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
    pub fn from_slice(data: &[u8]) -> Self {
        assert!(data.len() < 60);
        let mut p = Self {
            count: data.len() as u8,
            data: [0; MAX_PACKET_SIZE],
        };
        for i in 0..data.len() {
            p.data[i] = data[i];
        }
        p
    }
}

pub type CommChannel = Channel<ThreadModeRawMutex, CommPacket, 1>;

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
        let mut usb_buf = [0; 64];
        loop {
            let usb_reader = class.read_packet(&mut usb_buf);
            let ic_reader = self.usb_up.receive();

            match select(usb_reader, ic_reader).await {
                Either::First(bytes) => {
                    match bytes {
                        Ok(bytes) => {
                            defmt::info!("USB RX: {} {:?}", bytes, &usb_buf[0..bytes]);
                            // TODO: Check synchronization bytes!
                            let msg = CommPacket::from_slice(&usb_buf[0..bytes]);
                            if self.usb_down.try_send(msg).is_err() {
                                defmt::warn!("Unable to send received text upstream - is anyone listening? q_len={}", self.usb_down.len());
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
                    defmt::info!("USB TX: {:?}", msg);
                    /* If == 64, then zero-length packet later could be required. */
                    // class.write_packet(&ic_buf[0..bytes]).await?;
                    let mut buf: [u8; MAX_PACKET_SIZE] = [0; MAX_PACKET_SIZE];
                    buf[0] = 0x21; // !
                    buf[1] = 0x7C; // |
                    buf[2] = msg.count;
                    buf[3..3 + msg.count as usize].copy_from_slice(&msg.data[0..msg.count as usize]);
                    let buf = &buf[0..3 + msg.count as usize];

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
            // device_descriptor,
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
