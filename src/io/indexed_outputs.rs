use crate::io::events::{GroupedOutputs, IoIdx};
use embedded_hal::digital::OutputPin;

pub(crate) struct IndexedOutputs<
    const INDICES_N: usize,
    const EXPANDER_N: usize,
    const NATIVE_N: usize,
    ET: GroupedOutputs,
    P: OutputPin,
> {
    /// Numerical indices of given input/outputs - a unified mapping.
    indices: [u8; INDICES_N],
    /// Current known output status (true - high, false - low)
    state: [bool; INDICES_N],
    /// IO Expanders (16-bit PCF*)
    grouped: [ET; EXPANDER_N],
    /// Native pins.
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
            state: [false; IN],
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

    /// Read output state as we set it (doesn't read the PIN state).
    pub fn get(&self, io_idx: IoIdx) -> Option<bool> {
        Some(self.state[self.find_id(io_idx)?])
    }

    /// Toggle output and state. Return new state.
    pub async fn toggle(&mut self, io_idx: IoIdx) -> Result<bool, ()> {
        let position = self.find_id(io_idx).ok_or(())?;

        let current = self.state[position];
        self.set(io_idx, !current).await?;
        Ok(!current)
    }

    /// Set output based on IO index.
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
                self.state[position] = high;
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
                    expander.set_high(io_within).await?
                } else {
                    expander.set_low(io_within).await?
                }
                self.state[position] = high;
            }
            Ok(())
        } else {
            defmt::error!("Unable to find output with ID {}", io_idx);
            Err(())
        }
    }
}
