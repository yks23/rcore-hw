# AGENTS.md

## Cursor Cloud specific instructions

### Overview

This is a homework repository for the **rCore Tutorial** (Tsinghua University 2025S). It builds a RISC-V OS kernel in Rust that runs on QEMU. The actual source code lives in two git submodules (`code/` and `test/`) that point to Tsinghua's private GitLab. In Cloud Agent environments, these submodules cannot be cloned directly — use the GitHub mirrors instead (see below).

### Populating submodules from GitHub mirrors

The `.gitmodules` references `git@git.tsinghua.edu.cn` which is inaccessible in cloud environments. Populate the code and test directories from public GitHub mirrors:

```bash
git clone https://github.com/LearningOS/rCore-Tutorial-Code-2025S.git /tmp/rcore-code
git clone https://github.com/LearningOS/rCore-Tutorial-Test-2025S.git /tmp/rcore-test
rm -rf /workspace/code/*
cp -r /tmp/rcore-code/* /tmp/rcore-code/.* /workspace/code/ 2>/dev/null
cp -r /tmp/rcore-test/* /tmp/rcore-test/.* /workspace/test/ 2>/dev/null
```

Then checkout the desired chapter branch and set up the test suite:

```bash
cd /workspace/code
git checkout ch3   # or ch1-ch8
cp -r /tmp/rcore-test /workspace/code/user
```

### Bootloader compatibility (QEMU 8.x)

The `rustsbi-qemu.bin` shipped in the LearningOS fork **does not work** with QEMU 8.x (produces no output). Replace it with the one from the upstream `rcore-os/rCore-Tutorial-v3` repository:

```bash
git clone --depth=1 --branch ch3 https://github.com/rcore-os/rCore-Tutorial-v3.git /tmp/rcore-upstream
cp /tmp/rcore-upstream/bootloader/rustsbi-qemu.bin /workspace/code/bootloader/rustsbi-qemu.bin
```

### Build and run

```bash
cd /workspace/code/os
make build OFFLINE=1   # builds user apps + kernel
make run               # runs in QEMU (use timeout to avoid hanging on shutdown)
```

The OS will boot, run user programs, print results, and then repeatedly panic with "It should shutdown!" — this is expected behavior in ch3 because the SBI shutdown call doesn't cleanly exit QEMU 8.x. Use `timeout 20 make run` to avoid infinite hang.

### Lint

- `cd /workspace/code/os && cargo fmt -- --check` — passes on the OS kernel
- `cd /workspace/code/os && cargo clippy --target riscv64gc-unknown-none-elf` — has a pre-existing `missing_safety_doc` error due to `#![deny(warnings)]`
- `cd /workspace/code/user && cargo fmt -- --check` — has pre-existing formatting diffs in the test suite

### Key tools required

- Rust nightly (`nightly-2024-05-02` per `rust-toolchain.toml`) with `riscv64gc-unknown-none-elf` target
- `cargo-binutils` (install with `--locked` flag due to MSRV constraints)
- `qemu-system-riscv64` (apt package `qemu-system-misc`)
- Python 3, GNU Make
