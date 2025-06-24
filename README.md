IO Controller for SmartEnough
-----------------------------

IO Controller sits in your electrical box, reads inputs (switches) and changes
state of relays (lights or other things).

- System is distributed for reliability (and shorter power cables).
- CAN BUS communication between modules.
- Read inputs via PCF IO expander.
- Controls outputs via PCF IO expander.
- Default configuration assumes 16 switch inputs, 16 outputs, 16 sensor inputs
  (alarm, windows, etc.) per board.
- Tested on STM32G431. Default board assumes minimal soldering skills and reuses
  ready and cheap modules that are available to buy.
- Assumes generic twisted pair cable and connectors


Parts
-----

Default board assumes minimal soldering skills and reuses
ready and cheap modules that are available to buy:
- STM32G431 module by WeAct
- a 12V dc-dc converter,
- 5V dc-dc converter,
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
- Use recent stable Rust.
- Embassy links might need updating in Cargo.toml.
- Devices differ in their address (and maybe some functions later), pass
  appropriate feature during build, eg.:

      cargo build --release --bin ctrl --features bus-addr-1
      cargo build --release --bin gate --features bus-addr-gate
