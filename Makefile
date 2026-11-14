KERNEL = target/aarch64-unknown-none-softfloat/release/alba

.PHONY: build run clean

build:
	cargo build --release

run: build
	qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -kernel $(KERNEL)

clean:
	cargo clean
