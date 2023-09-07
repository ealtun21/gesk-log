# Define Rust standard targets for Linux, Windows, macOS, etc.
RUST_TARGETS := \
	x86_64-unknown-linux-musl \
	x86_64-pc-windows-gnu \
	i686-pc-windows-gnu \
	aarch64-unknown-linux-musl \
	arm-unknown-linux-musleabi \
	arm-unknown-linux-musleabihf \
	armv7-unknown-linux-musleabihf \
	i686-unknown-linux-musl

# Make all targets by default
all: $(RUST_TARGETS)

# Rule for building each target
$(RUST_TARGETS):
	@echo "Building for target: $@"
	@cross build --release --target=$@
	@if [ -f "target/$@/release/gesk-log" ]; then \
		cp target/$@/release/gesk-log gesk-log_$@; \
	fi
	@if [ -f "target/$@/release/gesk-log.exe" ]; then \
		cp target/$@/release/gesk-log.exe gesk-log_$@.exe; \
	fi

# Rule for cleaning up
clean:
	@echo "Cleaning up..."
	@rm -rf target

.PHONY: all clean $(RUST_TARGETS)
