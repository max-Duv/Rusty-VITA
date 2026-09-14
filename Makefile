.PHONY: build test check doctor probe live native clean

build:
	cargo build --release

test:
	cargo test

check:
	cargo fmt -- --check
	cargo check
	cargo test

doctor:
	./scripts/doctor_rs5.sh

probe: build
	./scripts/probe_bx01.sh 5

live: build
	./scripts/run_bx01.sh

native:
	cargo build --release --features native-pcap

clean:
	cargo clean
