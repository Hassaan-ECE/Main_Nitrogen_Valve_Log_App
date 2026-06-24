# AGENTS.md

Read this file first when working in this repository.

## Project Role

Desktop app for manually logging **main nitrogen valve** open/close events. The operator panel should stay simple, bold, and lab-friendly, with one clear primary action that alternates between **Close Valve** and **Open Valve** based on the latest saved manual log entry.

Design and stack are intentionally aligned with the PDU Data Automation App at:

```text
C:\Projects\Active\PDU_Data_Automation_App
```

## Current Status

`v0.1.7` — temporary manual open/close valve logging with local AppData logs and S-drive shared sync (`shared\events\` source of truth; `state.json` is best-effort cache).

Implemented:

- Tauri 2 desktop window (360×500, min 360×400)
- React 19 + TypeScript + Vite + Tailwind CSS v4 frontend
- One large alternating **Close Valve** / **Open Valve** workflow with PDU-style dark theme
- Startup reads the latest JSONL log entry; if no valid entry exists, it assumes/shows the valve as currently open
- PDU-style operator-name prompt with saved names, filtering, removal, blank validation, and case-insensitive dedupe
- Rust backend commands for current-state lookup, manual close logging, manual open logging, and opening the log workbook
- Logs live in `%APPDATA%\valve-log\logs\` (`events.jsonl` + `Main Nitrogen Valve Log.xlsx`) on each PC. S-drive `shared\` holds `state.json` and per-client `events\{client_id}\` for multi-machine sync (watcher on `shared\` only). Clear log data manually per PC when needed.

Not implemented yet:

- Hardware / PLC / serial / GPIO communication
- Settings, status polling, or error handling for real devices
- Installer signing, updater, or release pipeline (NSIS build exists; updater deferred)
- Multi-machine S-drive smoke test

## High-Priority Constraints

- **Use Bun** for installs and scripts. This project has `bun.lock` — do not switch back to npm unless Bun is broken and document why.
- **Preserve the PDU visual language** unless the user asks for a redesign: dark shell `#20201f`, green on `#1d7f47`, gray off `#343434`, Segoe UI, cyan focus/active rings.
- **Keep the operator panel simple.** Avoid adding complexity (extra screens, nav, settings drawers) unless the user requests it.
- **Do not claim hardware control or hardware state detection works** until a real integration path is implemented and tested. The current workflow is manual logging only.
- **Update agent docs after meaningful changes** — see [Keeping Docs Current](#keeping-docs-current).

## Preferred Stack

| Layer | Technology |
|-------|------------|
| Desktop shell | Tauri 2 (Rust) |
| UI | React 19, TypeScript, Vite |
| Styling | Tailwind CSS v4 |
| Package manager | Bun 1.3.14+ |
| Installer target | Tauri NSIS (when releases are added) |

## Quick Commands

```bash
bun install          # once per clone / after dependency changes
bun run desktop      # try the full app (recommended)
bun run dev          # frontend-only in browser
bun run build        # frontend production build
bun run build:desktop
```

**Windows note:** Tauri scripts use `bun --cwd backend ../node_modules/@tauri-apps/cli/tauri.js` because `cd backend && bunx tauri` fails when run through `bun run` on Windows.

## Architecture Direction

Separate concerns as the app grows:

- **Frontend** — manual operator workflow, prompt state, saved-name dropdown, status text, future log/history display.
- **Rust backend** — command dispatch, timestamps, JSONL/Excel logging, future device I/O, config.
- **Config** — connection settings, log paths, hardware profiles (add when needed under `config/`).

Current flow is manual: React loads the latest saved JSONL state -> button click -> operator prompt -> Tauri invoke -> Rust validates the open/close transition -> Rust appends JSONL -> Rust regenerates/opens Excel -> React updates the displayed manual state. This does not control or read physical valve hardware.

## Key Files

| Path | Purpose |
|------|---------|
| `frontend/src/features/valve-panel/ValvePanel.tsx` | Main manual open/close valve UI |
| `frontend/src/features/valve-panel/operatorNames.ts` | Saved operator-name helpers |
| `frontend/src/features/valve-panel/stateStyles.ts` | PDU-derived color palette |
| `frontend/src/integrations/tauri/valveLog.ts` | Frontend wrappers for state/log commands |
| `frontend/src/app/index.css` | Global theme / font |
| `backend/tauri.conf.json` | Window size, build hooks, bundle config |
| `backend/src/lib.rs` | Tauri entry and command registration |
| `backend/src/commands.rs` | Tauri command handlers |
| `backend/src/valve_log.rs` | JSONL state source, open/close transition validation, Excel workbook generation, log opener |
| `package.json` | Bun scripts and dependencies |
| `docs/AGENT_CHANGELOG.md` | Cross-agent change log — **update this** |
| `docs/PROJECT_STRUCTURE.md` | Directory map |
| `docs/ARCHITECTURE.md` | Stack and design details |

## Keeping Docs Current

When you make a **meaningful change** (new feature, stack change, new command, design shift, migration, broken/fixed workflow):

1. Append a dated entry to `docs/AGENT_CHANGELOG.md` with what changed, why, and how to verify.
2. Update `AGENTS.md` **Current Status** if the project phase changed.
3. Update `README.md` if user-facing run/build steps changed.
4. Update `docs/ARCHITECTURE.md` or `docs/PROJECT_STRUCTURE.md` if layout or data flow changed.

If another agent already updated the changelog since you started, read it before editing to avoid contradicting recent work.

## Validation Expectations

- Run `bun run build` after frontend changes.
- Run `bun run desktop` (or at least `bun run tauri -- --version`) after Tauri/script changes.
- Run `cargo check` in `backend/` after Rust changes.
- Do not claim desktop/hardware behavior works unless you actually ran it.

## Related Projects

- **PDU Data Automation App** — design reference and stack template: `C:\Projects\Active\PDU_Data_Automation_App`
- **PDU legacy scripts** — not in scope here unless the user connects this valve to that workflow

## Open Questions (as of v0.1.0)

Track answers in `docs/AGENT_CHANGELOG.md` when resolved:

- What hardware/interface controls the nitrogen valve?
- Is a single valve enough, or will multiple valves/channels be needed?
