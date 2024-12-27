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

use super::interconnect::Interconnect;

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

const MAX_PACKET_SIZE: u16 = 64;

pub type CommChannel = Channel<ThreadModeRawMutex, [u8; MAX_PACKET_SIZE as usize], 1>;

struct UsbProtocol {}

impl UsbProtocol {
    fn new() -> Self {
        Self {}
    }

    /// Connection spawner / manager.
    async fn connector(&self, class: &mut MyClass, interconnect: &Interconnect) -> ! {
        loop {
            info!("Awaiting connection in the connector");
            class.wait_connection().await;
            info!("Connected");
            let _ = self.forwarder(class, interconnect).await;
            info!("Disconnected");
        }
    }

    /// Connection handler
    async fn forwarder(
        &self,
        class: &mut MyClass,
        interconnect: &Interconnect,
    ) -> Result<(), Disconnected> {
        let mut usb_buf = [0; 64];
        loop {
            let usb_reader = class.read_packet(&mut usb_buf);
            let ic_reader = interconnect.receive();

            match select(usb_reader, ic_reader).await {
                Either::First(bytes) => {
                    if let Ok(bytes) = bytes {
                        defmt::info!(
                            "RX USB -> TX interconnect {} {:?}",
                            bytes,
                            &usb_buf[0..bytes]
                        );
                        // interconnect.write(&usb_buf[0..bytes]).await;
                    } else {
                        defmt::info!("Not ok!");
                    }
                }
                Either::Second(msg) => {
                    defmt::info!("RX interconnect -> TX USB {:?}", msg);
                    /* If == 64, then zero-length packet later could be required. */
                    // class.write_packet(&ic_buf[0..bytes]).await?;
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
        let class = CdcAcmClass::new(&mut builder, state, MAX_PACKET_SIZE);

        // Build the builder.
        let usb = builder.build();

        Self { usb, class }
    }

    pub async fn run(mut self, interconnect: &Interconnect) {
        let usb = self.usb.run();
        let protocol = UsbProtocol::new();
        let connector_future = protocol.connector(&mut self.class, interconnect);

        info!("Started USB");
        join(usb, connector_future).await;
    }
}
