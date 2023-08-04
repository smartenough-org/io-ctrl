
use defmt::info;
use embassy_stm32::usb_otg::{Driver, Instance};
use embassy_stm32::{bind_interrupts, peripherals, usb_otg, Config};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Builder;
use embassy_stm32::peripherals::USB_OTG_FS;
use embassy_stm32::peripherals::{PA12, PA11};
use embassy_usb::UsbDevice;
use embassy_futures::join::join;
use static_cell::make_static;


struct Disconnected {}

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
//type DevUsb<'d> = UsbDevice<'d, Driver<'d, USB_OTG_FS>>;
//type DevClass<'d> = CdcAcmClass<'d, Driver<'d, USB_OTG_FS>>;

struct UsbProtocol {
}

impl UsbProtocol {
    /// Connection spawner / manager.
    async fn connector(class: &mut MyClass) -> ! {
        loop {
            class.wait_connection().await;
            info!("Connected");
            let _ = UsbProtocol::echo(class).await;
            info!("Disconnected");
        }
    }

    /// Connection handler
    async fn echo(class: &mut MyClass) -> Result<(), Disconnected> {
        let mut buf = [0; 64];
        loop {
            let n = class.read_packet(&mut buf).await?;
            let data = &buf[..n];
            info!("data: {:x}", data);
            class.write_packet(data).await?;
        }
    }
}

pub struct UsbSerial {
    // usb: UsbDevice<'d, Driver<'d, USB_OTG_FS>>,
    // class: CdcAcmClass<'d, Driver<'d, USB_OTG_FS>>
    usb: MyUsb,
    class: MyClass,
}

bind_interrupts!(struct Irqs {
    OTG_FS => usb_otg::InterruptHandler<peripherals::USB_OTG_FS>;
});

impl UsbSerial {
    pub fn new(usb_peripheral: USB_OTG_FS,
               dp: PA12,
               dm: PA11) -> Self {
        // pub fn new(sth: (), pina) -> Self {
        // Create the driver, from the HAL.
        let mut ep_out_buffer = make_static!([0u8; 256]);
        let mut config = embassy_stm32::usb_otg::Config::default();
        config.vbus_detection = true;
        let driver = Driver::new_fs(usb_peripheral, Irqs, dp, dm,
                                    ep_out_buffer, config);
        // let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, &mut ep_out_buffer, config);

        // Create embassy-usb Config
        let mut config = embassy_usb::Config::new(0xd10d, 0x10de);
        config.manufacturer = Some("Alpk");
        config.product = Some("USB-USB communication diode");
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

        // Run the USB device.
        Self {
            usb,
            class,
        }
    }

    pub async fn run_loop(&mut self) {
        let usb = self.usb.run();
        let connector_future = UsbProtocol::connector(&mut self.class);

        join(usb, connector_future).await;
    }
}
