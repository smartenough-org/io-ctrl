
use defmt::info;
use embassy_stm32::usb_otg::Driver;
use embassy_stm32::{bind_interrupts, peripherals, usb_otg};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Builder;
use embassy_stm32::peripherals::USB_OTG_FS;
use embassy_stm32::peripherals::{PA12, PA11};
use embassy_usb::UsbDevice;
use embassy_futures::join::join;
use static_cell::make_static;
use crate::status::{Status, Message};
use crate::intercom::Intercom;

struct Disconnected;

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

type MyDriver = Driver<'static, USB_OTG_FS>;
type MyUsb = UsbDevice<'static, MyDriver>;
type MyClass = CdcAcmClass<'static, MyDriver>;

struct UsbProtocol {
    status: &'static Status
}

impl UsbProtocol {
    fn new(status: &'static Status) -> Self {
        Self {
            status
        }
    }

    /// Connection spawner / manager.
    async fn connector(&self, class: &mut MyClass, intercom: &impl Intercom) -> ! {
        loop {
            info!("Awaiting connection in the connector");
            class.wait_connection().await;
            info!("Connected");
            self.status.set_state(Message::Attention, 2).await;
            let _ = self.echo(class, intercom).await;
            info!("Disconnected");
            self.status.set_state(Message::Attention, 1).await;
        }
    }

    /// Connection handler
    async fn echo(&self, class: &mut MyClass, intercom: &impl Intercom) -> Result<(), Disconnected> {
        let mut buf = [0; 64];
        loop {
            let n = class.read_packet(&mut buf).await?;
            /* Maybe less often? */
            self.status.try_set_state(Message::Transfer, 1);
            let data = &buf[..n];
            info!("data: {:x}", data);
            class.write_packet(data).await?;
        }
    }
}

/// Device initialization.
pub struct UsbSerial {
    usb: MyUsb,
    class: MyClass,
    status: &'static Status,
}

bind_interrupts!(struct Irqs {
    OTG_FS => usb_otg::InterruptHandler<peripherals::USB_OTG_FS>;
});

impl UsbSerial {
    pub fn new(status: &'static Status,
               usb_peripheral: USB_OTG_FS,
               dp: PA12,
               dm: PA11) -> Self {
        // TODO: Maybe pull dp down for reenumeration on flash?

        // Create the driver, from the HAL.
        let ep_out_buffer = make_static!([0u8; 256]);
        let mut config = embassy_stm32::usb_otg::Config::default();

        // Setting to true requires additional connection to specific PIN
        config.vbus_detection = false;

        let driver = Driver::new_fs(usb_peripheral, Irqs, dp, dm,
                                    ep_out_buffer, config);

        // Create embassy-usb Config
        let mut config = embassy_usb::Config::new(0xd10d, 0x10de);
        config.manufacturer = Some("bla");
        config.product = Some("USB->USB communication diode");
        config.serial_number = Some("0000001");

        // Required for windows compatibility.
        // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
        config.device_class = 0xEF;
        config.device_sub_class = 0x02;
        config.device_protocol = 0x01;
        config.composite_with_iads = true;

        // Create embassy-usb DeviceBuilder using the driver and config.
        // It needs some buffers for building the descriptors.
        let device_descriptor = make_static!([0; 256]);
        let config_descriptor = make_static!([0; 256]);
        let bos_descriptor = make_static!([0; 256]);
        let control_buf = make_static!([0; 64]);

        let state = make_static!(State::new());

        let mut builder = Builder::new(
            driver,
            config,
            device_descriptor,
            config_descriptor,
            bos_descriptor,
            control_buf,
        );

        // Create classes on the builder.
        let class = CdcAcmClass::new(&mut builder, state, 64);

        // Build the builder.
        let usb = builder.build();

        Self {
            usb,
            class,
            status,
        }
    }

    pub async fn run(mut self, intercom: &impl Intercom) {
        let usb = self.usb.run();
        let protocol = UsbProtocol::new(self.status);
        let connector_future = protocol.connector(&mut self.class, intercom);

        info!("Started USB");
        join(usb, connector_future).await;
    }
}
