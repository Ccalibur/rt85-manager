name := "rt85-manager"
appid := "dev.rt85.Manager"
prefix := "/usr"

default: build-release

build-release:
    cargo build --release --workspace

run *args:
    RUST_LOG=rt85=debug cargo run -p rt85-gui -- {{args}}

cli *args:
    cargo run -p rt85-cli -- {{args}}

check:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

install:
    install -Dm0755 target/release/{{name}} {{prefix}}/bin/{{name}}
    install -Dm0755 target/release/rt85 {{prefix}}/bin/rt85
    install -Dm0644 resources/{{appid}}.desktop {{prefix}}/share/applications/{{appid}}.desktop
    install -Dm0644 resources/{{appid}}.metainfo.xml {{prefix}}/share/metainfo/{{appid}}.metainfo.xml
    install -Dm0644 resources/icons/hicolor/scalable/apps/{{appid}}.svg {{prefix}}/share/icons/hicolor/scalable/apps/{{appid}}.svg
    install -Dm0644 resources/udev/70-epomaker-rt85.rules /etc/udev/rules.d/70-epomaker-rt85.rules
