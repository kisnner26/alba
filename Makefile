TARGET  = aarch64-unknown-none-softfloat
ELF     = target/$(TARGET)/release/alba
BIN     = target/$(TARGET)/release/alba.bin
DISK    = target/alba-disk.img
QEMU    = qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic \
          -global virtio-mmio.force-legacy=false \
          -drive file=$(DISK),if=none,format=raw,id=hd0 -device virtio-blk-device,drive=hd0

.PHONY: build disk run test clean

# la imagen plana lleva la cabecera arm64: QEMU entrega el DTB en x0
build:
	cargo build --release
	python3 tools/mkimage.py $(ELF) $(BIN)

# disco albafs con los archivos de disk/ y los programas de usuario
disk:
	./tools/mkdisk.sh $(DISK)

run: build disk
	$(QEMU) -kernel $(BIN)

test:
	./tests/boot.sh

clean:
	cargo clean
