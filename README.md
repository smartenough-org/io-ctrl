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
- Assumes generic twisted pair cable (CAT5 UTP, maybe STP) and RJ45 connectors.

See (main repo)[https://github.com/smartenough-org/smartenough] for some images
of the board and other media.

Parts
-----
Default board assumes minimal soldering skills and reuses ready and cheap
modules that are available to buy. You can swap with other available modules,
but if using PCB take pinouts into account.

Main parts:
- 1x STM32G431 module by WeAct (or STM32G474 which has compatible pinout and more RAM)
  <https://github.com/WeActStudio/WeActStudio.STM32G431CoreBoard.git>,
- 1x 13-40V to 12V step-down 1-3A DC-DC converter (eg. LM2596, depends how high you
  drive you power bus. 24V or up to 30V it's easy to find parts),
- 1x 12V to 5V step-down 1-2A DC-DC converter (eg. MP1584),
- 1x CAN transceiver (eg. TJA1050),
- 1-3x PCF8575 16-bit IO expander module (can work with 8-bit version),
- PCB.

Varia:
- Pin headers (2.54mm) (male and female) of various sizes,
- Diodes (reverse connection protection),
- Fuse (short, over current protection for boards and bus),
- Power supply, switched-mode 16-30V,
- Twisted-pair cable between the nodes.

Things to control:
- 1-2x Some actuators: SSR or relay module (16-channels or less). Low-level trigger,
- Optional: RS485 interface,
- Optional: Additional EMP protection (TVS diodes) for inputs.

Cost estimate
-------------
Price as of 2026.02 for above main components for one board with 3x16IO is 45PLN.
Let's assume "varia" is another 45PLN. I've ordered 5 PCBs for 160PLN - 32PLN
per board if doing 5. That's 122PLN (30EUR, 35USD) for 48 low-voltage IOs.

16-channel active-low relays "10A" cost 36zł. That's 72PLN, 17EUR, 20USD.

So for a total of 200PLN (50EUR, 55USD) you get 32 230V-capable outputs and 16
inputs to read switches. Around (4.2PLN, 1.1EUR, 1.15USD) per IO. Less if you
include additional 24 native IOs (0.7EUR).

Comparison: Another great open source solution (boneio) that is commercially
available has a module with 32x 10A outputs and 49x digital inputs for 456EUR -
about 5.6EUR per IO (and that's not a bad price and you could do their module
yourself too).

Another completely commercial solution has 22x inputs + 22x outputs (although
not universal) for 7.8 EUR/IO, but without any build-in relays which are sold
separately for 9.72EUR per output. So that totals 12.6EUR/IO if counted in the
same way.

So it can be 7-12x times cheaper if done yourself.

Notes on design
---------------
- Cheap relays FAIL. But if you have more you can just swap channels. After a
  year I've got three 100% fine 16ch modules, and one with two failed channels
  which I swapped for other relays on the same module. For controlling a water
  pump I use SSR module which works still fine.
- When using cheap components give them a large margin. Don't assume the relay
  can trully switch 10A under load 10 times per hour. It's perfectly fine for
  controlling LED lighting.
- SSRs are nice, but relays allow to wire them for controlling UP/DOWN shutters
  where you shouldn't power UP and DOWN at the same time.
- My inputs are not explicitly isolated, optocoupled or shielded. That's
  obviously bad design, but for over a year I had zero problems. Why I think
  it's ok:
  - Multiple star-topologies architecture allows cables to be shorter. If you
    run cables along motor cables there MIGHT be some problems though. You can
    use SFTP (shielded twisted pair) and ground them correctly - I haven't.
  - I use IO-expanders modules so I can swap them easily when need arises. I run
    them on 5V (while talking to 3.3V controller) to lower the noise and I use
    software debouncing so short spikes are filtered out.
  - PCF8575 datasheet promises ESD protection that exceeds JESD 22 (1000V
    charged-device model).
  - I prefer to keep native STM32 IOs within the electrical box.
  - Power must be protected with fuses on each board and at power source -
    mostly to protect cables.
  - CAN transceiver has some line protection but could use some additional ESD +
    choke components though. A better module would be perfect.

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
