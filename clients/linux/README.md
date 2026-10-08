# Kodosi for Linux

Native Qt client for Kodosi. The runtime and terminal integration are built from
`../../runtime` and `../../terminal/ghostty` in this checkout.

Requires Linux x86-64, a C++23 compiler, Rust, Python, D-Bus, the Qt system library
dependencies and `uv`. Bootstrap installs pinned Qt, CMake and Ninja locally.

From the repository root:

```sh
just linux-build
just linux-check
```

The client directory also provides `just configure`, `just test`, `just lint` and
release packaging commands. Build output stays under `build/` and dependencies
under `.tools/`.

See [PRODUCT.md](PRODUCT.md) and [DESIGN.md](DESIGN.md) for the Linux experience.
Kodosi is MIT licensed; third-party notices are under `packaging/licenses/`.
