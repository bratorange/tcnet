BIND_IP ?= 127.0.0.1
USB_DIR ?= .
APP_BUNDLE = /Applications/DJSimulator.app
BINARY    = $(APP_BUNDLE)/Contents/MacOS/DJSimulator
LUCHS_SOCKET ?= /tmp/egui-mcp-luchs.sock

.PHONY: build build-luchs run-simulator run-simulator-mcp stop-simulator run-luchs-mcp stop-luchs

build:
	cargo build --features simulator,mcp --bin simulator

build-luchs:
	cargo build --features luchs --bin luchs

# Build, update the .app bundle, and (re)launch the simulator.
# The .app wrapper gives the process a stable bundle ID (com.tcnet.djsimulator)
# so that the computer-use MCP can grant and screenshot it via ScreenCaptureKit.
run-simulator: build
	cp ./target/debug/simulator $(BINARY)
	-kill $$(pgrep DJSimulator) 2>/dev/null; true
	sleep 0.5
	LUCHS_PYTHON=$(LUCHS_PYTHON) open $(APP_BUNDLE) --args --bind-ip $(BIND_IP) --usb-dir $(USB_DIR)

stop-simulator:
	-kill $$(pgrep DJSimulator) 2>/dev/null; true

# Build and run the simulator as a plain binary (no .app wrapper needed for egui-mcp).
# The IPC socket is at /tmp/egui-mcp.sock — egui-mcp tools connect automatically.
# Inherits LUCHS_PYTHON (auto-resolved from env / ~/Python/all-in-one venv) so
# the beat-grid analyser (madmom) runs on every loaded track.
run-simulator-mcp: build
	-kill $$(pgrep simulator) 2>/dev/null; true
	sleep 0.3
	RUST_LOG=warn,tcnet=info LUCHS_PYTHON=$(LUCHS_PYTHON) \
	  ./target/debug/simulator --bind-ip $(BIND_IP) --usb-dir $(USB_DIR) &

# Build and run LUCHS as a plain binary, with its own egui-mcp socket so it can
# run alongside the simulator (which keeps the default /tmp/egui-mcp.sock).
# LUCHS_PYTHON (if set) selects the python interpreter the analysis helpers
# run under — point it at a venv with librosa + essentia installed to get
# real M/P + pitch contour data.
LUCHS_PYTHON ?= python3

run-luchs-mcp: build-luchs
	-kill $$(pgrep luchs) 2>/dev/null; true
	-rm -f $(LUCHS_SOCKET)
	sleep 0.3
	RUST_LOG=warn EGUI_MCP_SOCKET=$(LUCHS_SOCKET) LUCHS_PYTHON=$(LUCHS_PYTHON) \
	  ./target/debug/luchs --bind-ip $(BIND_IP) --media-dir $(USB_DIR) &

stop-luchs:
	-kill $$(pgrep luchs) 2>/dev/null; true
