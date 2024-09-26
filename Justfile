set shell := ["nu", "-c"]

inject payload *args:
    cargo build --release --package {{ payload }}
    cargo run --bin tiny-pinch --release -- target/release/{{ payload }}.dll -- {{ args }}
