IO Controller for SmartEnough
-----------------------------

IO Controller sits in your electrical box, reads inputs (switches or sensors)
and changes state of relays (lights or other things). It transmits all his
events over the CAN bus and can be controlled over it.

- System is distributed for reliability (and shorter power cables).
- CAN Bus is used for communication between controllers and to connect it to a
  computer (eg. Homeassistant) with a Gate.
- Reads inputs via 16-bit PCF IO expander.
- Controls outputs via the same expander.
- Default configuration assumes 16 switch inputs, 16 outputs, 16 sensor inputs
  (alarm, windows, etc.) per board.
- Has additional support for shutters.
- Tested on STM32G431. Default board assumes minimal soldering skills and reuses
  ready and cheap modules that are available to buy.
- Assumes generic twisted pair cable and connectors

Parts
-----

Default board assumes minimal soldering skills and reuses
ready and cheap modules that are available to buy:
- STM32G431 module by WeAct
- a 13-40V to 12V dc-dc converter,
- 12V to 5V dc-dc converter,
- CAN interface,
- PCF 16-bit IO expander module,
- Actuators: SSR or relay module (16 channels or less). Low-level trigger,
- PCB (easy to order),
- Optional: RS485 interface,
- Optional: Additional EMP protection (TVS diodes) for inputs.

Varia:
- Pin headers (2.54mm) (male and female) of various sizes,
- Diodes (reverse connection protection),
- Fuse (short, over current protection for boards and bus),
- Power supply, switched-mode 16-30V.
- Twisted-pair cable between the nodes.


Building firmware
-----------------
- Use recent stable Rust, see rust-toolchain.toml.
- Embassy links might need updating in Cargo.toml.
- You can just `just` to run commands. Or see justfile for a set of example
  commands.
- Devices differ in their address (and maybe some functions later), pass
  appropriate feature during build, eg.:

      cargo install flip-link
      cargo install probe-rs-tools
      cargo build --release --bin ctrl --features bus-addr-1
      cargo build --release --bin gate --features bus-addr-gate
