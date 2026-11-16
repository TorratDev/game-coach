# game-coach

A Rust desktop AI game coach built on [Rune](https://github.com/anomalyco/rune): live game state, a deterministic rule engine, and LLM-narrated advice.

Status: **spike**. One libretro core's video in a Rune window with a live hex view of its system RAM. No audio yet.

Keys: arrows = d-pad, X = A, Z = B, A/S = L/R, Enter = Start, Backspace = Select.

```bash
cargo run -- path/to/mgba_libretro.so path/to/game.gba
```

- [docs/PLAN.md](docs/PLAN.md) — the original master plan
- [docs/REVIEW.md](docs/REVIEW.md) — critical architecture review and the recommended, much smaller plan
