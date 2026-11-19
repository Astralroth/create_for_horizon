# Create for Horizon

Rotating machines for [Horizon](https://github.com/horizon-rust/horizon): shafts,
cogwheels, motors and speed control. This is an independent project, written from
scratch as a native Horizon mod and inspired by the gameplay of the Create mod.
It contains no code or assets from Create or from other ports.

Status: package skeleton. It builds and loads, and does nothing else yet.

## Install

In Horizon Launcher (0.2.70 or later), open **Mods → Add from GitHub** and paste
`https://github.com/Astralroth/create_for_horizon`, then build the instance.
The mod is pinned to Horizon `0b14f8a` (nightly 2026-11-18) for Minecraft 26.3.

## Develop

`cargo horizon-mod check` validates the package against the pinned SDK.

## License

MIT, see [LICENSE](LICENSE).
