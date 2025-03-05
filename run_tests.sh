#!/bin/bash

# Try with arguments like `shutters`, `bindings`. Needs device to run tests on.
# Should not depend on a particular hardware unless noted otherwise in specific test.
cargo test --release --features bus-addr-1 --test main $*
