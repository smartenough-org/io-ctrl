mod common;
mod hardware;

pub mod ctrl_board_v1;

/// Select HW version here.
pub use ctrl_board_v1 as ctrl_board;
