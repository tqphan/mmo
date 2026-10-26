# 0007: Godot 4.7.x .NET client in C#

- **Status:** Accepted, 2026-10-25
- **Decision:** The client is Godot 4.7.x (.NET build) in C#, using the official `SpacetimeDB.ClientSDK.Godot` package. Bindings are generated into `client/net` by `cargo xtask generate`.
- **Consequences:** The machine running the client needs the .NET SDK. Formulas the client predicts come from `sim` and are checked against generated test vectors. Not yet installed on the Linux machine (see STATUS).
