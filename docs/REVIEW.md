# Critical Review of the Master Plan

Reviewed 2026-11-16 against [PLAN.md](PLAN.md). Verdict: **the product idea is fine; the architecture is about 5x larger than it needs to be, has one outright impossible data path, and contradicts its own compliance story.** Do not start Phase 0 as written.

Confidence labels: **[verified]** = checked against a source today; **[recall]** = from memory, confirm before relying on it.

## 1. Blockers (the plan does not work as written)

### 1.1 `adb forward` cannot carry UDP
The RetroArch pipeline is "UDP 55355 via ADB Forward". ADB forwarding is TCP-only (plus local/abstract sockets); there is no UDP forward. **[recall, long-standing ADB behaviour]** You would need the emulator console's `redir add udp:55355:55355`, or a TCP-to-UDP relay inside the guest. This is fixable, but it shows the pipeline was never tried end to end, and it is moot given 2.1.

### 1.2 The ARM64 translator is not something you can build or ship
The plan offers "AOSP `libndk_translation`" or "licensed/extracted Intel `houdini`".
- `libndk_translation` is not in AOSP source. It ships only as prebuilt blobs inside Google's emulator system images. The open translator in AOSP (Berberis) does not target ARM64-on-x86_64. **[recall]**
- Houdini is Intel-proprietary; "extracted" means redistributing binaries you have no licence for.

So the "custom AOSP image, distributed to end users" has no legal ARM64 story. Without ARM64 translation there is no Dofus Touch, and Dofus Touch is the only reason the Android VM exists.

### 1.3 "Sanitized fingerprint" is detection evasion, and it will not work anyway
Building `user`/release-keys and scrubbing `ro.kernel.qemu` to "present a standard Android tablet" exists only to hide the emulator from the game. That directly contradicts the risk matrix row claiming a read-only, compliant integration. It is also ineffective: detection looks at goldfish/ranchu devices, sensors, GL renderer strings, the presence of a native bridge, and attestation, not just build props. And the emulator's own HALs key off the qemu properties, so scrubbing them breaks the guest. **[recall]**

Combined with multi-account tabs, "cross-character turn coordination", and a `sendTouch` autonomy mode, the plan describes MMO multi-boxing bot infrastructure with a coach attached. A public repo makes that description readable by the publisher. Recommendation: drop fingerprint spoofing and Dofus input automation entirely; I would not build those parts.

### 1.4 Redistribution of `emulator.exe`
Google's emulator binaries come under the Android SDK licence, which restricts redistribution. The emulator is open source, so you can build your own, but a Windows build of the emulator is a project in itself. The installer in Phase 5 assumes this is free. **[recall, read the SDK licence]**

## 2. Architecture: wrong shape

### 2.1 RetroArch inside an Android VM inside Windows is absurd
You would run a SNES emulator inside an ARM-translated Android guest inside QEMU, then reach its memory over a UDP text protocol through a port redirect, polling at 100 ms. Alternatives, in order of preference:
1. **Host a libretro core directly in the Rust process.** The libretro API hands you `retro_get_memory_data` (direct WRAM pointer, zero latency), `retro_serialize` (savestates in microseconds, not a ~1 s VM snapshot), frame stepping, and input injection. No VM, no ADB, no UDP, no snapshots service.
2. Run RetroArch for Windows and use the same network command interface on localhost.

Either removes the VM from the entire retro half of the product.

### 2.2 This is two products sharing a sidebar
| | Retro RPG coach | Dofus Touch coach |
|---|---|---|
| State source | exact RAM | vision + OCR |
| Needs Android VM | no | yes |
| Needs ML training | no | yes |
| Autonomy legit | yes (offline) | no |
| Legal exposure | ROMs are user-supplied | ToS, emulator, translator blobs |

The "Unified GameState Store" is the only shared piece, and a Pokémon party and a Dofus isometric board have almost nothing in common to normalise. Build the retro coach first: it is deterministic, legal, testable in CI, and exercises the UI, rule engine and LLM tier. Decide on Dofus afterwards with real knowledge.

### 2.3 You do not need a custom AOSP build
An AOSP checkout is ~300 GB and hours per build, plus an image-distribution system (another `Must Define`). The only things the custom image buys are the fingerprint spoofing (drop it, 1.3) and pre-installed apps (use `adb install`). If the Dofus half survives, start with a stock Google emulator system image the user downloads through the SDK, or go further and **do not own the emulator at all**: capture the window of whatever emulator or mirrored device (scrcpy) the user already runs, and draw a transparent click-through overlay window on top. That is how game overlays normally work and it deletes the VMM supervisor, gRPC client, image pipeline, WHPX preflight and installer wizard.

### 2.4 The video path is not zero-copy and the latency target is unrealistic
`streamScreenshot` is a screenshot API. At 1920x1080 RGBA 60 FPS that is ~500 MB/s of pixels through gfxstream readback, shared memory, a CPU copy, and a texture upload; "zero-copy" is false on every hop. `< 35 ms input-to-photon` through that path, across a hypervisor and a translated ARM64 game, is a number with no measurement behind it.
If you do embed: Rune's `NativeSurface` on Windows accepts **D3D shared textures from another process** **[verified in `crates/rune/src/native_surface/mod.rs` on `dev`]**, which is the real zero-copy route, but nothing in the stock emulator exports one. Prove the frame path with a one-day spike before anything else is designed around it.

### 2.5 The vision design uses ML where geometry works
A Dofus-style combat map is a fixed isometric cell grid; cell centres are a closed-form function of screen coordinates at a canonical resolution. Training a detector to "map isometric grid cells" is wasted effort. Only entity occupancy needs perception, and even there, team-coloured ground circles and the turn timeline are template/colour matching problems before they are YOLO problems. The detector, its dataset and labelling workflow (a 4-week phase and a `Must Define`) should be the last resort, not the plan.
Also missing: the coach needs spell ranges, damage formulas and monster stats. Vision gives you positions, not the rules database. Where that data comes from is not mentioned anywhere.

### 2.6 The LLM tier is specified by latency, not by what it can know
A 3B-8B local model has no reliable knowledge of Dofus spells or Gen 3 damage calcs and will state wrong numbers confidently. The correct split: the deterministic engine does **all** computation (damage, lethal, reachable cells, best line via search); the LLM only turns that structured result into prose and post-fight summaries. Then the provider choice stops mattering much and the latency budget is cosmetic. Also unaddressed: a local LLM, DirectML vision and the game's own rendering all contend for one consumer GPU.

### 2.7 `ExecutionMode` as an enum is not a gate
In an open-source client, a mode flag is a one-line patch. If autonomy must be impossible on online games, the input-injection code must not be linked into the Dofus path at all (separate crate/feature, not compiled in). Under the 2.2 split this falls out naturally.

## 3. Smaller issues

- **AEHD**: Google has announced it is winding AEHD down **[recall, verify date]**; plan for WHPX only, which requires Hyper-V features many gaming PCs have disabled and which conflicts with some anti-cheat drivers.
- **Rune**: real and active (MIT, `dev` branch, pushed today, `rune-kit`/`rune-motion` exist) **[verified]**, but pre-release with video marked unsupported on some platforms. The plan's "DX12 `wgpu`" assumption should be checked on a Windows box; Rune has grown its own backends. Pinning a rev is the right call.
- **Schedule**: 18 weeks covers a custom Android distribution, a VM host, a streaming client, an ML dataset plus trained detector, OCR in two languages, a rules engine for multiple games, an LLM agent, multi-instance orchestration and a signed installer. Each `Must Define` is an unbounded research task parked in a 2-week phase. Realistic for a small team: retro coach in ~6-8 weeks; the rest is unknown until the spikes are done.
- **Exit criteria are not measurable**: "smoothly", "accurate", "real time". Give numbers and a test (e.g. board state matches ground truth on N recorded fights at >= X%).
- **No test strategy**: recorded-frame and recorded-RAM fixtures should exist from week 1; they are the only way to regression-test perception against game patches.
- **Hot-reloadable ROI JSON** treats patch drift as a config problem. Layout changes break models and templates, not just coordinates.
- **Multi-VM numbers** (16 GB for 1, 32 GB for 4) are guesses. Not needed if multi-account is dropped.

## 4. Recommended plan

1. **Spike (1 week, throwaway):** Rune window on Windows showing a libretro core's video via `NativeSurface` byte frames, with a live RAM read printed in a side panel. This validates Rune, the frame path and the state path at once.
2. **Retro coach v1:** in-process libretro core, TOML memory maps keyed by ROM hash, rule engine for one game (Pokémon Emerald or FireRed, pick one), drawer UI, savestate rewind. Fixture tests from recorded RAM dumps.
3. **LLM narration** over rule-engine output; user-supplied API key first, local model only if someone asks.
4. **Offline autonomy** for the retro core, in its own crate.
5. **Dofus decision point.** If still wanted: read-only overlay over a user-run emulator or mirrored device, geometry-first board reconstruction, no input injection, no fingerprint work, no custom image. Check the publisher's current terms on emulators and overlays first.

Deleted relative to the original: custom AOSP build, image distribution, VMM supervisor, emulator gRPC client, ADB bridge, guest daemon, fingerprint sanitising, multi-instance VMs, the YOLO grid detector, the Windows installer wizard.

## 5. `Must Define` items, resolved or removed

| Item | Disposition |
|---|---|
| AOSP base version, translation layer, image distribution, guest resolution | Removed with the custom image |
| Pinned Rune SHA | Keep; pin at spike time |
| Max concurrent VMs | Removed |
| Hotkeys | Defer; trivial |
| Game roster | One GBA Pokémon title for v1 |
| Memory schema | Custom TOML; import from RetroAchievements notes later if useful |
| Dofus dataset | Deferred to the decision point; likely unnecessary |
| LLM provider | User API key first |
| TTS | No |
| Autonomy enforcement | Compile-time separation, retro only |
