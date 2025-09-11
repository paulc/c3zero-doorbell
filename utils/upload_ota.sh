#!/bin/sh

EXAMPLE=${1:?Usage: $0 <example>}

cargo build --release --example "${EXAMPLE}" && \
    espflash save-image --chip esp32c3 "target/riscv32imc-esp-espidf/release/examples/${EXAMPLE}" "ota-bin/${EXAMPLE}.bin" && \
    curl -X POST --upload-file "ota-bin/${EXAMPLE}.bin" http://192.168.60.102/ota
