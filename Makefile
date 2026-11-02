PLUGIN_DIR := $(HOME)/.config/zellij/plugins
TARGET := wasm32-wasip1

.PHONY: all build install clean

all: build

build:
	cargo build --release --target $(TARGET)

install: build
	install -d $(PLUGIN_DIR)
	install -m 644 target/$(TARGET)/release/tab-groups.wasm $(PLUGIN_DIR)/tab_groups.wasm
	install -m 644 target/$(TARGET)/release/tab-stats.wasm $(PLUGIN_DIR)/tab_stats.wasm
	install -m 644 target/$(TARGET)/release/context-rail.wasm $(PLUGIN_DIR)/context_rail.wasm

clean:
	cargo clean
