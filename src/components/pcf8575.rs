use embedded_hal_async::i2c;

/// Thin wrapper over PCF8575 module.
/// TODO: Handle INT line and read only when triggered. Here... or layer higher?
pub struct Pcf8575<BUS: i2c::I2c>
{
    /// Shared i2c bus
    i2c: BUS,

    /// I2C Port Expander address
    addr: u8,
}

impl<BUS: i2c::I2c> Pcf8575<BUS>
{
    pub fn new(i2c: BUS, a0: bool, a1: bool, a2: bool) -> Self {
        let addr = 0x20 | ((a2 as u8) << 2) | ((a1 as u8) << 1) | (a0 as u8);
        Self {
            i2c,
            addr,
        }
    }

    /// Byte order: port 0 (P07-P00), port 1 (P17-P10)
    pub async fn read(&mut self) -> Result<u16, ()> {
        let mut buf = [0, 0];
        self.i2c.read(self.addr, &mut buf).await.map_err(|_e| ())?;
        Ok(u16::from_le_bytes(buf))
    }

    pub async fn write(&mut self, data: u16) -> Result<(), ()> {
        let buf = data.to_le_bytes();
        self.i2c.write(self.addr, &buf).await.map_err(|_e| ())?;
        Ok(())
    }
}
