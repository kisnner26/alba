TARGET  = aarch64-unknown-none-softfloat
ELF     = target/$(TARGET)/release/alba
BIN     = target/$(TARGET)/release/alba.bin
QEMU    = qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic

.PHONY: build run test clean

# la imagen plana lleva la cabecera arm64: QEMU entrega el DTB en x0
build:
	cargo build --release
	python3 tools/mkimage.py $(ELF) $(BIN)

run: build
	$(QEMU) -kernel $(BIN)

test:
	./tests/boot.sh

clean:
	cargo clean
