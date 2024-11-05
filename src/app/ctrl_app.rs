use core::cell::UnsafeCell;

use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::uid;
use embassy_time::{Duration, Timer};

use crate::boards::ctrl_board::Board;
use crate::boards::OutputChannel;

use crate::buttonsmash::consts::BINDINGS_COUNT;
use crate::buttonsmash::{EventChannel, Executor, Opcode};
use crate::io::event_converter::run_event_converter;

/// High-level command queue that are produced by executor.
static EVENT_CHANNEL: EventChannel = EventChannel::new();
/// Command Queue that connects Executor and IO Router.
static OUTPUT_CHANNEL: OutputChannel = OutputChannel::new();

pub struct CtrlApp {
    /// For all IO needs (and comm peripherals like CAN and USB)
    pub board: &'static Board,
    executor: UnsafeCell<Executor<BINDINGS_COUNT>>,
}

impl CtrlApp {
    pub async fn new(board: &'static Board) -> Self {
        // let cmd_queue = CMD_QUEUE.init(CommandQueue::new());
        let mut executor = Executor::new(&OUTPUT_CHANNEL);
        Self::configure(&mut executor).await;

        Self {
            board,
            executor: UnsafeCell::new(executor),
        }
    }

    /// Returns hard-configured Executor. TODO: This is temporary.
    async fn configure(executor: &mut Executor<BINDINGS_COUNT>) {
        const PROGRAM: [Opcode; 25] = [
            // Setup proc.
            Opcode::Start(0),
            Opcode::LayerDefault,
            Opcode::BindShortToggle(1, 1),
            Opcode::BindShortToggle(2, 2),
            Opcode::BindShortToggle(3, 3),
            Opcode::BindShortToggle(4, 4),
            Opcode::BindShortToggle(5, 5),
            Opcode::BindShortToggle(6, 6),
            Opcode::BindShortToggle(7, 7),
            Opcode::BindShortToggle(8, 8),
            Opcode::BindShortToggle(9, 9),
            Opcode::BindShortToggle(10, 10),
            Opcode::BindShortToggle(11, 11),
            Opcode::BindShortToggle(12, 12),
            Opcode::BindShortToggle(13, 13),
            Opcode::BindShortToggle(14, 14),
            Opcode::BindShortToggle(15, 15),
            Opcode::BindShortToggle(16, 16),
            Opcode::Stop,
            /*
            Opcode::BindShortToggle(1, 10),
            Opcode::BindShortToggle(2, 11),
            Opcode::BindLongToggle(3, 20),
            Opcode::BindShortToggle(3, 21),
            Opcode::BindShortCall(4, 1),
            Opcode::BindLayerHold(5, 66),
            Opcode::LayerPush(66),
            Opcode::BindShortToggle(1, 13),
            */
            Opcode::Stop,
            // Test proc.
            Opcode::Start(1),
            Opcode::Activate(100),
            Opcode::Activate(101),
            Opcode::Deactivate(110),
            Opcode::Stop,
        ];

        executor.load_static(&PROGRAM).await;
    }

    fn spawn_tasks(&'static self, spawner: &Spawner) {
        let executor = unsafe { &mut *self.executor.get() };
        unwrap!(spawner.spawn(task_pump_switch_events_to_microvm(executor)));
        unwrap!(spawner.spawn(run_event_converter(self.board.input_q, &EVENT_CHANNEL)));
    }

    pub async fn main(&'static mut self, spawner: &Spawner) -> ! {
        self.spawn_tasks(spawner);

        defmt::info!("Starting app on chip {}", uid::uid());
        loop {
            // defmt::info!("Main app tick");
            Timer::after(Duration::from_millis(1000)).await;
            self.board.led_on();
            Timer::after(Duration::from_millis(1000)).await;
            self.board.led_off();

            /*
            let ir_reg = pac::FDCAN1.ir().read();
            let cccr_reg = pac::FDCAN1.cccr().read();
            let psr_reg = pac::FDCAN1.cccr().read();

            defmt::info!("APP cccr={:b} DAR={} init={} | ir={:b} psr={:b} pea={} ped={} bo={} ew={} ep={} tcf={} mraf={}",
                         cccr_reg.0, cccr_reg.dar(), cccr_reg.init(),

                         ir_reg.0, psr_reg.0, ir_reg.pea(), ir_reg.ped(), ir_reg.bo(),
                         ir_reg.ew(), ir_reg.ep(), ir_reg.tcf(), ir_reg.mraf());
            */
        }
    }
}


#[embassy_executor::task(pool_size = 1)]
pub async fn task_pump_switch_events_to_microvm(executor: &'static mut Executor<BINDINGS_COUNT>) {
    executor.listen_events(&EVENT_CHANNEL).await;
}
