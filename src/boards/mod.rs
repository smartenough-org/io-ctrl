mod common;
mod io_router;

pub mod ctrl_board_v1;

/// Select HW version here.
pub use ctrl_board_v1 as ctrl_board;

pub use io_router::{IOCommand, OutputChannel};
