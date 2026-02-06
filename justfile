# For measure-stack; also see .cargo/config.toml for cargo run configuration.
chip := "STM32G431CBUx"
# Default CAN ctrl board address
addr := "bus-addr-1"
probe_gold_desk := "0483:3748:C28C6D010110134753384C4E"
probe_violet_desk := "0483:3748:6C65090132124647524B4E"
# Common build args. To fit on µC use --release always. See Cargo.toml
buildargs := "--release"

build bin addr:
    cargo build {{ buildargs }} --bin {{bin}} --features {{addr}}

# Build board controller
build-ctrl: (build "ctrl" addr)
    #cargo build {{ buildargs }} --bin ctrl --features {{addr}}

# Build CAN<->USB gate
build-gate: (build "gate" "bus-addr-gate")

# Build gate and default controller
build-all: build-ctrl build-gate

# Build while allowing for easy listing of errors from top.
build-less:
    cargo lbuild {{ buildargs }} --bin ctrl --features {{addr}} --color=always 2>&1 | less

clippy bin="ctrl":
    cargo clippy {{ buildargs }} --bin {{bin}} --features {{addr}}

run-ctrl:
    #cargo run --bin ctrl --features bus-addr-1 -- --probe {{probe_gold_desk}} --always-print-stacktrace $*
    cargo run {{ buildargs }} --bin ctrl --features {{addr}} -- --probe {{probe_gold_desk}} --always-print-stacktrace

run-gate:
    cargo run {{ buildargs }} --bin gate --features bus-addr-gate -- --probe {{probe_violet_desk}}

# This uses probe-run (not newer probe-rs) to measure stack usage.
measure-stack probe="C28C6D010110134753384C4E00": build-ctrl
    # probe-run has different probe format than probe-rs.
    probe-run -v --measure-stack --chip {{ chip }} target/thumbv7em-none-eabi/release/ctrl --probe {{ probe }} --preverify

# Measure object sizes (flash usage)
measure: build-ctrl build-gate
    arm-none-eabi-objcopy -O binary ./target/thumbv7em-none-eabi/release/ctrl _tempfile.tmp
    @echo "Size of controller:"
    @ls -l ./target/thumbv7em-none-eabi/release/ctrl _tempfile.tmp
    @echo
    arm-none-eabi-objcopy -O binary ./target/thumbv7em-none-eabi/release/gate _tempfile.tmp
    @echo "Size of gate:"
    @ls -l ./target/thumbv7em-none-eabi/release/gate _tempfile.tmp
    @rm -f _tempfile.tmp

format:
    cargo fmt


# mode: makefile
