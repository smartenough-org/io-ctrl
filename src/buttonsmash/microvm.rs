/*
 * TODO: We lack the ability to toggle a group on/off if say one lamp from the group is
 * already enabled.
*/

use defmt::Format;
use embassy_time::{Duration, Timer};

use super::bindings::*;
use super::consts::{
    Command, Event, EventChannel, InIdx, MAX_LAYERS, MAX_PROCEDURES, MAX_STACK, OutIdx, ProcIdx,
    REGISTERS,
};
use super::{layers::Layers, opcodes::Opcode, shutters};
use crate::boards::ctrl_board_v1::Board;
use crate::components::interconnect::WhenFull;
use crate::components::status;
use crate::components::{
    interconnect::Interconnect,
    message::{Message, args},
};
use crate::io::events::Trigger;

/// MicroVM holds internal state that can be queried by code.
/// TODO Output status migrated to Board. So now this is WIP.
pub struct BoardState {
    /// TODO: In progress.
    registers: [u8; REGISTERS],
}

impl Default for BoardState {
    fn default() -> Self {
        Self {
            registers: [0; REGISTERS],
        }
    }
}

/// Executes actions using a program.
pub struct Executor<const BINDINGS: usize, const OPCODES: usize = 1024> {
    layers: Layers,
    bindings: BindingList<BINDINGS>,
    opcodes: [Opcode; OPCODES],
    procedures: [usize; MAX_PROCEDURES],
    // Cached state of the board and VM registers/state.
    state: BoardState,

    // Our outputs
    board: &'static Board,
    shutters: &'static shutters::ShutterChannel,
}

enum MicroState {
    /// Continue execution
    Continue,
    /// Stop execution
    Stop,
    /// Call a subprocedure using a call stack
    CallProc(usize),
    // Jump to opcode (no stack change)
    // Jump(usize),
}

#[derive(Debug, Eq, PartialEq, Format, Clone)]
pub enum IOCommand {
    /// Toggle output...
    ToggleOutput(OutIdx),
    /// Enable output of given ID - Local or remote.
    ActivateOutput(OutIdx),
    /// Deactivate output of given ID - Local or remote
    DeactivateOutput(OutIdx),
}

impl<const BN: usize> Executor<BN> {
    pub fn new(
        board: &'static Board,
        shutters_addr: &'static shutters::ShutterChannel,
    ) -> Self {
        Self {
            layers: Layers::new(),
            bindings: BindingList::new(),
            opcodes: [Opcode::Noop; 1024],
            procedures: [0; MAX_PROCEDURES],
            state: BoardState::default(),
            board,
            shutters: shutters_addr,
        }
    }

    pub async fn load_static(&mut self, program: &[Opcode]) {
        for (idx, opcode) in program.iter().enumerate() {
            self.opcodes[idx] = *opcode;
        }
        self.index_code();
        self.execute(0).await;
        // Finish on default layer
        self.layers.reset();
    }

    /// Broadcast our output state change
    async fn emit_io_message(&mut self, out: OutIdx, final_state: bool) {
        defmt::info!(
            "Emiting IO message for output {} to state {} from executor",
            out,
            final_state
        );

        // TODO: I've mixed feeling about handling this in emit(). Move lower
        // and create emit_message and emit_io?

        let message = Message::OutputChanged {
            output: out,
            state: if final_state {
                args::OutputChangeRequest::On
            } else {
                args::OutputChangeRequest::Off
            },
        };

        // Transmit information over CAN.
        // In case of broken CAN communication this will be ignored.
        self.board.interconnect.transmit_response(&message, WhenFull::Drop).await;
    }

    /// Handle outputs from Executor: Emit two messages and change internal state.
    async fn alter_output(&mut self, command: IOCommand) {
        // Update local state
        let (result, out) = match &command {
            IOCommand::ToggleOutput(out) => (self.board.toggle_output(*out).await, *out),
            IOCommand::ActivateOutput(out) => {
                (self.board.set_output(*out, true).await.map(|()| true), *out)
            }
            IOCommand::DeactivateOutput(out) => (
                self.board.set_output(*out, false).await.map(|()| false),
                *out,
            ),
        };

        if let Ok(final_state) = result {
            defmt::info!("Executor changed output state {:?}", command);
            self.emit_io_message(out, final_state).await;
        } else {
            defmt::error!("Error while setting output {:?}", command);
            status::COUNTERS.expander_output_error.inc();
            // TODO: Send error message over CAN?
        }
    }

    /// Send MASS status info.
    async fn send_status(&mut self) {
        let status = self.board.get_output_status().await;
        for (idx, state) in status {
            let state = if state {
                args::IOState::On
            } else {
                args::IOState::Off
            };
            let message = Message::StatusIO {
                io: args::IOType::Output(idx),
                state,
            };
            // Transmit information over CAN.
            defmt::info!("Sent status message {:?}", message);
            self.board.interconnect.transmit_response(&message, WhenFull::Wait).await;

            // Don't block on CAN in case it died (we are alone on bus for
            // example), but give it some time to send. On 250kBps frame should
            // take < 0.6ms.
            Timer::after(Duration::from_millis(1)).await;
        }

        for exp in [&self.board.expander_sensors, &self.board.expander_switches] {
            let inputs = exp.get_inputs();
            if let Some(inputs) = inputs {
                for (idx, state) in inputs {
                    let state = if state {
                        args::IOState::On
                    } else {
                        args::IOState::Off
                    };
                    let message = Message::StatusIO {
                        io: args::IOType::Input(idx),
                        state,
                    };
                    // Transmit information over CAN.
                    defmt::info!("Sent status input message {:?}", message);
                    self.board.interconnect.transmit_response(&message, WhenFull::Wait).await;
                }
            } else {
                for idx in exp.get_indices() {
                    let message = Message::StatusIO {
                        io: args::IOType::Input(*idx),
                        state: args::IOState::Error,
                    };
                    self.board.interconnect.transmit_response(&message, WhenFull::Wait).await;
                }
                defmt::info!(
                    "Expander id={} does not respond. Dead: {:?}",
                    exp.get_id(),
                    exp.get_indices()
                );
            }
        }

        // TODO: Send global warning/error status as well.
    }

    /// Helper: Bind input/trigger to a call to a given procedure.
    fn bind_proc(&mut self, idx: InIdx, trigger: Trigger, proc_idx: ProcIdx) {
        self.bindings.bind(Binding {
            idx,
            trigger,
            layer: self.layers.current,
            action: Action::Proc(proc_idx),
        });
    }

    /// Helper: Bind input/trigger to single command.
    fn bind_single(&mut self, idx: InIdx, trigger: Trigger, command: Command) {
        self.bindings.bind(Binding {
            idx,
            trigger,
            layer: self.layers.current,
            action: Action::Single(command),
        });
    }

    async fn execute_opcode(&mut self, opcode: Opcode) -> MicroState {
        match opcode {
            Opcode::Noop => { /* Noop */ }
            Opcode::Stop => {
                return MicroState::Stop;
            }
            Opcode::Start(_) => {
                panic!("Invalid opcode: Start");
            }
            Opcode::Call(proc_id) => {
                return MicroState::CallProc(proc_id as usize);
            }
            /*
            Opcode::CallToggle(register, proc_id_true, proc_id_false) => {
                if self.registers[register] {
                    // Internal register was true, toggle it and call first procedure.
                    // Used for grouping.
                    self.registers[register] = false;
                    return MicroState::CallProc(proc_id_true as usize);
                } else {
                    self.registers[register] = true;
                    return MicroState::CallProc(proc_id_false as usize);
                }
            }
            */
            Opcode::CallRegister(register) => {
                return MicroState::CallProc(self.state.registers[register as usize] as usize);
            }
            Opcode::SetRegister(register, value) => {
                self.state.registers[register as usize] = value;
            }
            Opcode::Toggle(out_idx) => {
                self.alter_output(IOCommand::ToggleOutput(out_idx)).await;
            }
            Opcode::Activate(out_idx) => {
                self.alter_output(IOCommand::ActivateOutput(out_idx)).await;
            }
            Opcode::Deactivate(out_idx) => {
                self.alter_output(IOCommand::DeactivateOutput(out_idx))
                    .await;
            }

            // Enable a layer (TODO: push layer onto a layer stack?)
            Opcode::LayerPush(layer) => {
                assert!(layer as usize <= MAX_LAYERS);
                // Use a `virtual` input idx of 0 when forcing a layer activation.
                self.layers.activate(0, layer);
            }
            Opcode::LayerPop => {
                // Deactivate last virtual 0 input.
                self.layers.maybe_deactivate(0);
            }
            Opcode::LayerSet(layer) => {
                self.layers.reset();
                self.layers.activate(0, layer);
            }

            // Clear the layer stack - back to default layer.
            Opcode::LayerDefault => {
                self.layers.reset();
            }

            // WaitForRelease - maybe?
            // Procedure 0 is executed after loading and it can map the actions initially

            // Clear all the bindings.
            Opcode::BindClearAll => {
                self.bindings.clear();
            }

            Opcode::BindShortCall(switch_id, proc_idx) => {
                self.bind_proc(switch_id, Trigger::ShortClick, proc_idx);
            }
            Opcode::BindLongCall(switch_id, proc_idx) => {
                self.bind_proc(switch_id, Trigger::LongClick, proc_idx);
            }
            Opcode::BindActivateCall(switch_id, proc_idx) => {
                self.bind_proc(switch_id, Trigger::Activated, proc_idx);
            }
            Opcode::BindDeactivateCall(switch_id, proc_idx) => {
                self.bind_proc(switch_id, Trigger::Deactivated, proc_idx);
            }
            Opcode::BindLongActivate(switch_id, proc_idx) => {
                self.bind_proc(switch_id, Trigger::LongActivated, proc_idx);
            }
            Opcode::BindLongDeactivate(switch_id, proc_idx) => {
                self.bind_proc(switch_id, Trigger::LongDeactivated, proc_idx);
            }

            /*
             * Shortcuts
             */
            // Trivial configuration shortcuts.
            Opcode::BindShortToggle(switch_id, out_idx) => {
                self.bind_single(
                    switch_id,
                    Trigger::ShortClick,
                    Command::ToggleOutput(out_idx),
                );
            }

            Opcode::BindLongToggle(switch_id, out_idx) => {
                self.bind_single(
                    switch_id,
                    Trigger::LongClick,
                    Command::ToggleOutput(out_idx),
                );
            }

            Opcode::BindLayerHold(switch_id, layer_idx) => {
                // When this is in use + ShortClick is defined for the same key,
                // then the shortclick should be defined on new layer.
                self.bind_single(
                    switch_id,
                    Trigger::Activated,
                    Command::ActivateLayer(layer_idx),
                );

                // NOTE: Layer deactivation is handled automatically and should
                // not be bound.
            }
            Opcode::BindShutter(shutter_idx, down_idx, up_idx) => {
                self.shutters
                    .send((shutter_idx, shutters::Cmd::SetIO(down_idx, up_idx)))
                    .await;
            }

            Opcode::SendStatus => {
                self.send_status().await;
            } // Hypothetical?
              // Read input value (local) into register
              /*
                   Opcode::ReadInput(switch_id) => {
               },
                   /// Read input value (local) into register
                   Opcode::ReadOutput(OutIdx) => {
               },
                   /// Call first if register is True, second one if False.
                   Opcode::CallConditionally(proc_idx, proc_idx) => {
               },
              */
        }
        MicroState::Continue
    }

    pub async fn execute(&mut self, proc: ProcIdx) {
        let mut pc = self.procedures[proc as usize];

        // We start with an empty stack. First procedure doesn't need an entry.
        let mut stack: [usize; MAX_STACK] = [0; MAX_STACK];
        let mut stack_idx = 0;

        assert_eq!(self.opcodes[pc], Opcode::Start(proc));
        loop {
            pc += 1;
            let opcode = self.opcodes[pc];
            match self.execute_opcode(opcode).await {
                MicroState::Continue => {}
                MicroState::Stop => {
                    if stack_idx == 0 {
                        // Nothing to return to. Finish execution.
                        break;
                    }
                    stack_idx -= 1;
                    pc = stack[stack_idx];
                }
                MicroState::CallProc(proc_id) => {
                    // Check for overflow.
                    if stack_idx == MAX_STACK {
                        defmt::panic!("Stack overflow! ptr={} stack={}", stack_idx, stack);
                    }
                    stack[stack_idx] = pc;
                    stack_idx += 1;
                    pc = self.procedures[proc_id];
                    // pc points to Start now and will be incremented.
                }
            }
        }
    }

    /// Index procedures' starts
    fn index_code(&mut self) {
        for i in 0..MAX_PROCEDURES {
            self.procedures[i] = 0;
        }

        for (idx, opcode) in self.opcodes.iter().enumerate() {
            if let Opcode::Start(proc_idx) = opcode {
                self.procedures[*proc_idx as usize] = idx;
            }
        }
    }

    /// Reads events and reacts to it.
    pub async fn parse_event(&mut self, event: Event) {
        match event {
            // Local button press.
            Event::ButtonEvent(data) => {
                if data.trigger == Trigger::Deactivated
                    && self.layers.maybe_deactivate(data.switch_id)
                {
                    // Deactivated layer that was previously activated using
                    // this key. TODO: Warning! Event order might be important.
                    // longclick, longdeactivate first, then deactivate?
                    return;
                }

                let binding = self.bindings.filter(
                    data.switch_id,
                    Some(self.layers.current),
                    Some(data.trigger),
                );
                if let Some(binding) = binding {
                    match binding.action {
                        Action::Noop => {}
                        Action::Single(cmd) => match cmd {
                            Command::ActivateLayer(layer) => {
                                self.layers.activate(data.switch_id, layer);
                            }
                            Command::DeactivateLayer(_layer) => {
                                todo!("deactivation is based on stack list");
                            }
                            Command::Noop => {}
                            Command::ToggleOutput(out) => {
                                self.alter_output(IOCommand::ToggleOutput(out)).await;
                            }
                            Command::ActivateOutput(out) => {
                                self.alter_output(IOCommand::ActivateOutput(out)).await;
                            }
                            Command::DeactivateOutput(out) => {
                                self.alter_output(IOCommand::DeactivateOutput(out)).await;
                            }
                            Command::Shutter(shutter_idx, cmd) => {
                                self.shutters.send((shutter_idx, cmd)).await;
                            }
                        },
                        Action::Proc(proc_idx) => {
                            self.execute(proc_idx).await;
                        }
                    }
                } else {
                    defmt::info!("Not found binding {:?}!", data);
                }

                // Now, since the local (fast) action is executed, broadcast the
                // input change.
                let msg = Message::InputChanged {
                    input: data.switch_id,
                    trigger: data.trigger
                };
                self.board.interconnect.transmit_response(&msg, WhenFull::Wait).await;
            }
            // Remote call over Interconnect.
            Event::RemoteProcedureCall(proc_idx) => {
                self.execute(proc_idx).await;
            }
            Event::RemoteToggle(out_idx) => {
                self.alter_output(IOCommand::ToggleOutput(out_idx)).await;
            }
            Event::RemoteActivate(out_idx) => {
                self.alter_output(IOCommand::ActivateOutput(out_idx)).await;
            }
            Event::RemoteDeactivate(out_idx) => {
                self.alter_output(IOCommand::DeactivateOutput(out_idx))
                    .await;
            }
            Event::RemoteStatusRequest => {
                self.send_status().await;
            }
        }
    }

    pub async fn listen_events(&mut self, event_channel: &'static EventChannel) {
        loop {
            let input_event = event_channel.receive().await;
            self.parse_event(input_event).await;
        }
    }
}
