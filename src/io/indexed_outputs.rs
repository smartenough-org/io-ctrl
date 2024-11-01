use crate::io::events::{GroupedOutputs, IoIdx};
use embedded_hal::digital::OutputPin;

pub(crate) struct IndexedOutputs<
    const INDICES_N: usize,
    const EXPANDER_N: usize,
    const NATIVE_N: usize,
    ET: GroupedOutputs,
    P: OutputPin,
> {
    indices: [u8; INDICES_N],
    grouped: [ET; EXPANDER_N],
    native: [P; NATIVE_N],
}

impl<const IN: usize, const EN: usize, const NN: usize, ET: GroupedOutputs, P: OutputPin>
    IndexedOutputs<IN, EN, NN, ET, P>
{
    /// Create new indexed output mapping with few expanders (16 IOs each) and any number of native Pins.
    /// Passed indices list maps any numeric ID to each of the PINs.
    //
    // MAYBE: Make indices tuple to index into native-0, or expander ID.
    pub fn new(grouped: [ET; EN], native: [P; NN], indices: [u8; IN]) -> Self {
        IndexedOutputs {
            grouped,
            native,
            indices,
        }
    }

    /// Find IO Index within the list.
    /// TODO: Optimise by sorting in-place a tuple list?
    fn find_id(&self, io_idx: IoIdx) -> Option<usize> {
        for (pos, cur_io_idx) in self.indices.iter().enumerate() {
            if *cur_io_idx == io_idx {
                return Some(pos);
            }
        }
        None
    }

    pub async fn set(&mut self, io_idx: IoIdx, high: bool) -> Result<(), ()> {
        if let Some(position) = self.find_id(io_idx) {
            let expander_no = position / 16;
            if expander_no > self.grouped.len() {
                // That indexes into native PIN
                let native_pos = position - (expander_no * 16);
                if high {
                    self.native[native_pos].set_high().unwrap();
                } else {
                    self.native[native_pos].set_low().unwrap();
                }
                return Ok(());
            } else {
                let expander = &mut self.grouped[expander_no];
                let io_within = position - expander_no * 16;
                if io_within >= 16 {
                    defmt::panic!("Calculated IO within expander is invalid");
                }
                let io_within = io_within as u8;
                // TODO: This unwrap will kill program if there's no IO to be set (no PCF)
                if high {
                    expander.set_high(io_within).await.unwrap();
                } else {
                    expander.set_low(io_within).await.unwrap();
                }
            }

            Ok(())
        } else {
            defmt::error!("Unable to find output with ID {}", io_idx);
            Err(())
        }
    }

    pub async fn set_high(&mut self, idx: IoIdx) -> Result<(), ()> {
        self.set(idx, true).await
    }

    pub async fn set_low(&mut self, idx: IoIdx) -> Result<(), ()> {
        self.set(idx, false).await
    }
}
