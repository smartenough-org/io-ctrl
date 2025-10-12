use embassy_stm32::can;

use crate::buttonsmash::{
    shutters,
    consts::{InIdx, OutIdx, ProcIdx, ShutterIdx}
};

/* Generic CAN has 11-bit addresses.
 * - Messages must be unique
 * - Lower values have higher priorities.
 * - We want up to 64 devices on the bus.
 * - This gives 6 bit for device address and 5 for message type, ie. 32 different messages
 * TTTTTAAAAAA (T)ype + (A)ddress
 */

/// The lower the code, the more important the message on the CAN BUS.
mod msg_type {
    // Start with rare important events.
    // Range: 5 bits, 0x00 <-> 0x1f

    // 0 Reserved as invalid message
    // 1 Reserved for high-priority grouped type.

    /// Erroneous situation happened. Includes error code. See Info/Warning
    pub const ERROR: u8 = 0x02;

    // 3 reserved

    /// My output was changed, because of reasons.
    pub const OUTPUT_CHANGED: u8 = 0x04;
    /// My input was changed.
    pub const INPUT_TRIGGERED: u8 = 0x05;

    /// Set output X to Y (or invert state)
    pub const SET_OUTPUT: u8 = 0x08;
    /// Simulate input trigger, just like if the user presses the button.
    pub const TRIGGER_INPUT: u8 = 0x09;
    /// Call a predefined procedure in VM.
    pub const CALL_PROC: u8 = 0x0A;
    /// Extended set (shutters, etc)
    pub const CALL_SHUTTER: u8 = 0x0B;

    /// `Ping` of sorts.
    pub const REQUEST_STATUS: u8 = 0x0D;

    /// Periodic not triggered by an event status.
    pub const STATUS: u8 = 0x10;
    pub const TIME_ANNOUNCEMENT: u8 = 0x11;

    /// Similar to Error but with low priority.
    /// eg. Device started
    pub const INFO: u8 = 0x12;

    /*
    /// TODO: We will need something for OTA config updates.
    /// To whom this may concern (device ID), total length of OTA
    pub const MICROCODE_UPDATE_INIT: u8 = 0x1C;
    /// Part of binary code for upgrade.
    pub const MICROCODE_UPDATE_PART: u8 = 0x1A;
    /// CRC, apply if matches.
    pub const MICROCODE_UPDATE_END: u8 = 0x1B;
    */
    pub const PONG: u8 = 0x1D;
    pub const PING: u8 = 0x1E;

    // 0x1F Reserved for low-priority grouped type
}

pub mod args {
    pub use crate::io::events::Trigger;

    #[derive(Clone, Copy, defmt::Format)]
    #[repr(u16)]
    pub enum InfoCode {
        Started = 10,
    }

    #[derive(Clone, Copy, defmt::Format)]
    #[repr(u8)]
    pub enum OutputState {
        Off = 0,
        On = 1,
        Toggle = 2,
        // on for x?
    }

    impl InfoCode {
        pub fn to_bytes(self) -> u16 {
            self as u16
        }
    }

    impl OutputState {
        pub fn to_bytes(self) -> u8 {
            self as u8
        }

        pub fn from_u8(raw: u8) -> Option<Self> {
            match raw {
                0 => Some(Self::Off),
                1 => Some(Self::On),
                2 => Some(Self::Toggle),
                _ => {
                    defmt::warn!("OutputState parsed from invalid arg {}", raw);
                    None
                }
            }
        }
    }

    impl Trigger {
        pub fn to_bytes(self) -> u8 {
            self as u8
        }

        pub fn from_u8(raw: u8) -> Option<Self> {
            match raw {
                0 => Some(Trigger::ShortClick),
                1 => Some(Trigger::LongClick),
                2 => Some(Trigger::Activated),
                3 => Some(Trigger::Deactivated),
                4 => Some(Trigger::LongActivated),
                5 => Some(Trigger::LongDeactivated),
                _ => None
            }
        }
    }
}

/// This holds the decoded message internally.
#[derive(defmt::Format)]
pub enum Message {
    // Start with rare important events.
    /// Erroneous situation happened. Includes error code.
    Error { code: u32 },
    /// Normal or slightly weird situation happened (eg. initialized)
    Info { code: u16, arg: u32 },

    /// My output was changed.
    OutputChanged {
        output: OutIdx,
        state: args::OutputState,
    },

    /// My input was changed.
    InputTriggered { input: InIdx },

    /// Request output change.
    /// 0 - deactivate, 1 - activate, 2 - toggle, * reserved (eg. time-limited setting)
    SetOutput {
        output: OutIdx,
        state: args::OutputState,
    },

    // Behave as if input was triggered
    TriggerInput {
        input: InIdx,
        trigger: args::Trigger,
    },

    ShutterCmd {
        shutter_idx: ShutterIdx,
        cmd: shutters::Cmd,
    },

    /// Better Ping. TODO: Handle RTR?
    RequestStatus,
    /// Initial Ping that has some simple data to return in Pong.
    Ping { body: u16 },
    /// Response to Ping.
    Pong { body: u16 },

    /// Periodic not triggered by event status.
    Status {
        uptime: u32,
        inputs: u16,
        outputs: u16,
    },

    /// Sent to endpoints.
    TimeAnnouncement {
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        day_of_week: u8,
    },

    /// Call local procedure
    CallProcedure { proc_id: ProcIdx },
    /* TODO
    /// TODO: We will need something for OTA config updates.
    /// To whom this may concern (device ID), total length of OTA
    MicrocodeUpdateInit {
    addr: u8,
    length: u32,
    },
    /// Part of binary code for upgrade.
    MicrocodeUpdatePart {
    // Cycling offset.
    offset: u16,
    chunk: [u8; 6],
    },
    /// CRC, apply if matches.
    MicrocodeUpdateEnd {
    chunks: u16,
    length: u32,
    crc: u16,
    },
    /// Microcode received and applied.
    MicrocodeUpdateAck {
    length: u32,
    }
     */
}

/// Raw message prepared for sending or just received.
#[derive(defmt::Format, Default)]
pub struct MessageRaw {
    /// "Device" address - either source (for responses/status), or destination (for requests)
    addr: u8,
    msg_type: u8,

    length: u8,
    data: [u8; 8],
}

impl MessageRaw {
    pub fn from_bytes(addr: u8, msg_type: u8, data: &[u8]) -> Self {
        let mut raw = Self {
            addr,
            msg_type,
            length: data.len() as u8,
            data: [0; 8],
        };
        raw.data[0..data.len()].copy_from_slice(data);
        raw
    }

    /// Reconstruct from received data.
    pub fn from_can(can_addr: u16, data: &[u8]) -> Self {
        let (msg_type, addr) = Self::split_can_addr(can_addr);
        let mut raw = Self {
            addr,
            msg_type,
            length: data.len() as u8,
            data: [0; 8],
        };
        raw.data[0..data.len()].copy_from_slice(data);
        raw
    }

    pub fn to_can_frame(&self) -> can::frame::Frame {
        let standard_id = embedded_can::StandardId::new(self.to_can_addr())
            .expect("This should create a message");
        let id = embedded_can::Id::Standard(standard_id);
        let hdr = can::frame::Header::new(id, self.length(), false);
        can::frame::Frame::new(hdr, self.data_as_slice()).unwrap()
    }

    /// Combine parts into 11-bit CAN address.
    pub fn to_can_addr(&self) -> u16 {
        ((self.msg_type as u16 & 0x1F) << 6) | (self.addr as u16 & 0x3F)
    }

    /// Split/parse 11 bit CAN address into msg type and device address
    pub fn split_can_addr(can_addr: u16) -> (u8, u8) {
        let device_addr: u8 = (can_addr & 0x3F).try_into().unwrap();
        let msg_type: u8 = ((can_addr >> 6) & 0x1F).try_into().unwrap();
        (msg_type, device_addr)
    }

    pub fn addr_type(&self) -> (u8, u8) {
        (self.addr, self.msg_type)
    }

    pub fn length(&self) -> u8 {
        self.length
    }

    pub fn data_as_slice(&self) -> &[u8] {
        &self.data[0..self.length as usize]
    }
}

impl Message {
    pub fn from_raw(raw: &MessageRaw) -> Option<Self> {
        match raw.msg_type {
            msg_type::SET_OUTPUT => {
                if raw.length != 2 {
                    defmt::warn!("Set output has invalid message length {:?}", raw);
                    return None;
                }

                let state = args::OutputState::from_u8(raw.data[1])?;
                Some(Message::SetOutput {
                    output: raw.data[0],
                    state,
                })
            }
            msg_type::TRIGGER_INPUT => {
                if raw.length != 2 {
                    defmt::warn!("Trigger input has an invalid message length {:?}", raw);
                    return None;
                }

                let trigger = args::Trigger::from_u8(raw.data[1])?;
                Some(Message::TriggerInput {
                    input: raw.data[0],
                    trigger,
                })
            }
            msg_type::CALL_PROC => {
                if raw.length != 1 {
                    defmt::warn!("Call proc has invalid message length {:?}", raw);
                    return None;
                }
                let proc_id: ProcIdx = raw.data[0];
                Some(Message::CallProcedure { proc_id })
            }
            msg_type::TIME_ANNOUNCEMENT => {
                if raw.length != 2 + 1 + 1 + 1 + 1 + 1 + 1 {
                    defmt::warn!("Time announcement has invalid message length {:?}", raw);
                    return None;
                }
                Some(Message::TimeAnnouncement {
                    year: u16::from_le_bytes([raw.data[0], raw.data[1]]),
                    month: raw.data[2],
                    day: raw.data[3],
                    hour: raw.data[4],
                    minute: raw.data[5],
                    second: raw.data[6],
                    day_of_week: raw.data[7],
                })
            }

            msg_type::REQUEST_STATUS => Some(Message::RequestStatus),

            msg_type::PING => Some(Message::Ping {
                body: u16::from_le_bytes([raw.data[0], raw.data[1]]),
            }),

            msg_type::PONG => Some(Message::Pong {
                body: u16::from_le_bytes([raw.data[0], raw.data[1]]),
            }),

            msg_type::INFO | msg_type::ERROR | msg_type::STATUS => {
                defmt::info!("Ignoring info/error/status message: {:?}", raw);
                None
            }

            msg_type::OUTPUT_CHANGED | msg_type::INPUT_TRIGGERED => {
                defmt::info!("Ignoring output/input change message {:?}", raw);
                None
            }

            _ => {
                // TBH, probably safe to ignore.
                defmt::warn!("Unable to parse unhandled message type {:?}", raw);
                None
            }
        }
    }

    /// Convert message to 11 bit address and up to 8 bytes of data to be sent via CAN.
    pub fn to_raw(&self, addr: u8) -> MessageRaw {
        let mut raw = MessageRaw {
            addr,
            ..MessageRaw::default()
        };

        match self {
            Message::Error { code } => {
                raw.msg_type = msg_type::ERROR;
                raw.length = 4;
                raw.data[0..4].copy_from_slice(&code.to_le_bytes());
            }
            Message::Info { code, arg } => {
                raw.msg_type = msg_type::INFO;
                raw.length = 6;
                raw.data[0..2].copy_from_slice(&code.to_le_bytes());
                raw.data[2..6].copy_from_slice(&arg.to_le_bytes());
            }
            Message::SetOutput { output, state } => {
                raw.msg_type = msg_type::SET_OUTPUT;
                raw.length = 2;
                raw.data[0] = *output;
                raw.data[1] = state.to_bytes();
            }
            Message::OutputChanged { output, state } => {
                raw.msg_type = msg_type::OUTPUT_CHANGED;
                raw.length = 2;
                raw.data[0] = *output;
                raw.data[1] = state.to_bytes();
            }
            Message::InputTriggered { input } => {
                raw.msg_type = msg_type::INPUT_TRIGGERED;
                raw.length = 1;
                raw.data[0] = *input; // ? More?
            }
            Message::CallProcedure { proc_id } => {
                raw.msg_type = msg_type::CALL_PROC;
                raw.length = 1;
                raw.data[0] = *proc_id;
            }
            Message::ShutterCmd { shutter_idx, cmd } => {
                raw.msg_type = msg_type::CALL_SHUTTER;
                raw.length = 7;
                raw.data[0] = *shutter_idx;
                cmd.to_raw(&mut raw.data[1..6]);
            }
            Message::Status {
                uptime,
                inputs,
                outputs,
            } => {
                raw.msg_type = msg_type::STATUS;
                raw.length = 8;
                raw.data[0..4].copy_from_slice(&uptime.to_le_bytes());
                raw.data[4..6].copy_from_slice(&inputs.to_le_bytes());
                raw.data[6..8].copy_from_slice(&outputs.to_le_bytes());
            }
            Message::Ping { body } => {
                raw.msg_type = msg_type::PING;
                raw.length = 2;
                raw.data[0..2].copy_from_slice(&body.to_le_bytes());
            }
            Message::Pong { body } => {
                raw.msg_type = msg_type::PONG;
                raw.length = 2;
                raw.data[0..2].copy_from_slice(&body.to_le_bytes());
            }
            /* we only parse those.
            Message::TimeAnnouncement { year, month, day, hour, minute, second } => todo!(),
            Message::MicrocodeUpdateInit { addr, length } => todo!(),
            Message::MicrocodeUpdatePart { offset, chunk } => todo!(),
            Message::MicrocodeUpdateEnd { chunks, length, crc } => todo!(),
            Message::MicrocodeUpdateAck { length } => todo!(),
            */
            Message::TriggerInput { .. }
            | Message::RequestStatus { .. }
            | Message::TimeAnnouncement { .. } => {
                panic!("Not implemented method requested");
            }
        }
        raw
    }
}
