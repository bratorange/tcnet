BIND_IP ?= 127.0.0.1
USB_DIR ?= .
APP_BUNDLE = /Applications/DJSimulator.app
BINARY    = $(APP_BUNDLE)/Contents/MacOS/DJSimulator

.PHONY: build run-simulator run-simulator-mcp stop-simulator

build:
	cargo build --features simulator,mcp --bin simulator

# Build, update the .app bundle, and (re)launch the simulator.
# The .app wrapper gives the process a stable bundle ID (com.tcnet.djsimulator)
# so that the computer-use MCP can grant and screenshot it via ScreenCaptureKit.
run-simulator: build
	cp ./target/debug/simulator $(BINARY)
	-kill $$(pgrep DJSimulator) 2>/dev/null; true
	sleep 0.5
	open $(APP_BUNDLE) --args --bind-ip $(BIND_IP) --usb-dir $(USB_DIR)

stop-simulator:
	-kill $$(pgrep DJSimulator) 2>/dev/null; true

# Build and run the simulator as a plain binary (no .app wrapper needed for egui-mcp).
# The IPC socket is at /tmp/egui-mcp.sock — egui-mcp tools connect automatically.
run-simulator-mcp: build
	-kill $$(pgrep simulator) 2>/dev/null; true
	sleep 0.3
	RUST_LOG=warn ./target/debug/simulator --bind-ip $(BIND_IP) --usb-dir $(USB_DIR) &
