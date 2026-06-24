# Agent Changelog

Living log for AI agents (and humans) working on this repo. **Append new entries at the top.** Do not delete history.

Format:

```markdown
## YYYY-MM-DD — Short title (agent or author)

**What changed**
- ...

**Why**
- ...

**How to verify**
- ...

**Follow-ups**
- ...
```

---

## 2026-06-24 — v0.1.5: shared save failure no longer flips local state

**What changed**
- Bumped app metadata to `v0.1.5` for a replacement release.
- Changed connected-mode valve logging so the backend publishes to the shared `events\{client_id}\` file and shared `state.json` before appending the event to this PC's local JSONL cache.
- Added a regression test that forces a shared write failure after event preparation and confirms the local source log is not written.
- Added a `local_log_write_failed` saved-entry error path for the rare case where shared sync succeeds but this PC cannot update its local cache.

**Why**
- A failed shared write could previously leave a local-only JSONL event behind even though the UI showed "The event could not be saved to the shared valve log." That made the initiating PC's button flip after closing the prompt while other PCs stayed unchanged.

**How to verify**
- `cd backend && cargo check`
- `cd backend && cargo test`
- `bun run build`
- Note: `cargo fmt --check` still reports pre-existing formatting differences in `backend/src/shared_sync.rs`; only `backend/src/valve_log.rs` was formatted to keep this fix scoped.

**Follow-ups**
- Publish `v0.1.5` and remove old broken release artifacts.
- On the lab PCs, retry the failing save and confirm that a shared-save error no longer changes only one machine's local button state.
- If the footer still says Connected while saves fail, inspect S-drive write permissions for `shared\events\` and `shared\state.json`.

## 2026-06-24 — v0.1.4: one-time local log reset on update

**What changed**
- Added `backend/src/migrations.rs` — first launch of v0.1.4 deletes `%APPDATA%\valve-log\logs\` once (marker: `migrations\clear_local_logs_v0.1.4.done`).
- Migration runs at startup before state load so updated PCs drop stale local JSONL/Excel and reload from shared sync.
- Released v0.1.4; replaces v0.1.3 on GitHub and S-drive.

**Why**
- Admin can clear `shared\` on S-drive, users update, and local history is wiped automatically for a coordinated fresh start.

**How to verify**
- On two PCs with v0.1.3, log different events locally.
- Clear `shared\` on S-drive (or set desired shared state).
- Update both PCs to v0.1.4 via footer updater.
- Confirm `%APPDATA%\valve-log\logs\` is gone/recreated empty and both PCs show the same shared-derived state.

**Follow-ups**
- Remove migration in a future version once all lab PCs have passed through v0.1.4.

---

## 2026-06-24 — v0.1.3: revert to local AppData logs, keep shared sync

**What changed**
- Logs and Excel back in `%APPDATA%\valve-log\logs\` on each PC (reverts v0.1.2 S-drive authoritative `logs\` layout).
- Shared sync restored to v0.1.1 model: `shared\state.json` fast read, `shared\events\{client_id}\` merge, watcher on `shared\` only.
- Kept stale shared write-lock recovery (30s) from unreleased fix.
- Bumped to `v0.1.3`; removed `v0.1.2` release assets from repo and GitHub.

**Why**
- User prefers per-PC local logs cleared manually; S-drive authoritative logs were harder to manage safely.

**How to verify**
- `cd backend && cargo test`
- `bun run build`
- Install v0.1.3; confirm logs land in `%APPDATA%\valve-log\logs\`.
- With S drive up, log on one PC and confirm another updates via shared sync.
- GitHub `releases/latest` serves v0.1.3 only.

**Follow-ups**
- Manually clear AppData logs on PCs when resetting test data.

---

## 2026-06-24 — S-drive shared sync with local-first logging

**What changed**
- Added compact shared sync under `S:\Engineering\Public\Syed_Hassaan_Shah\Main_Nitrogen_Valve_Log_App\shared\`.
- Local durable log now lives in app data (`events.jsonl`); Excel stays local for speed.
- Shared `state.json` (~150 bytes) is the fast read path; per-event JSON files land in `shared/events/{client_id}/`.
- Merge replays open/close chronologically; duplicate opens keep the earlier entry, duplicate closes keep the later entry.
- Frontend polls every 500ms and listens for `valve-log:changed` so other machines update quickly.
- Override shared root with `NITROGEN_VALVE_LOG_SHARED_ROOT` when testing.

**Why**
- Multiple lab PCs need the same valve state immediately after someone logs open/close.
- Inventory-style FeOxDB op-log sync is heavier than needed for rare append-only valve events.

**How to verify**
- `cd backend && cargo test`
- `bun run build`
- With S drive available, log on one PC and confirm another PC button/state updates within ~500ms.
- Confirm `shared/state.json` and a new file under `shared/events/` after logging.
- Disconnect S drive, log locally, confirm warning text and `event_saved` behavior.

**Follow-ups**
- NSIS shortcut hooks and signed updater (deferred).
- Multi-machine smoke on real S-drive shares.

---## 2026-06-24 — Simplified and formatted Excel log export

**What changed**
- Excel workbook now exports only `Timestamp`, `Valve`, `Action`, and `Operator`.
- Removed Excel columns: `Previous State`, `New State`, `Source`, `Notes`.
- Action labels in new logs and Excel export are now `Closed Valve` and `Opened Valve`.
- Legacy JSONL rows with `Close Valve` / `Open Valve` are normalized when the workbook is regenerated.
- Added Excel formatting: styled header, borders, alternating rows, colored actions, frozen header, autofilter.

**Why**
- User wanted a cleaner operator-facing log without internal state/metadata columns.

**How to verify**
- `cd backend && cargo test`
- Log a close/open event, click **Open Log**, confirm Excel has 4 columns and formatted rows.
- Existing JSONL history should still regenerate into the slimmer workbook.

**Follow-ups**
- None for Excel layout unless operators request more columns.

## 2026-06-24 — Alternating manual open/close logging workflow

**What changed**
- Updated the close-only workflow into an alternating manual **Close Valve** / **Open Valve** workflow.
- Added `get_current_valve_state` so the app loads the latest valid JSONL entry on startup and shows the correct manual state.
- Added `log_valve_opened` while preserving `log_valve_closed`.
- Added backend transition checks so close is only allowed from latest open state, and open is only allowed from latest closed state.
- Kept JSONL as the durable source and Excel as the operator-facing workbook; workbook rows now include close and open notes.
- Updated the frontend Tauri wrapper and operator panel to keep one primary button that alternates after each successful manual log.
- Updated docs from close-only language to manual open/close valve logging and marked the old close-only implementation plan as historical.

**Why**
- Operators need to log both end-of-day valve close events and next-morning valve open events without implying hardware control.
- Startup state should survive app restarts by reading the latest saved manual log entry.

**How to verify**
- `bun run build` passed.
- `cd backend && cargo check` passed.
- `cd backend && cargo test` passed with 12 tests.
- `bun run desktop` was attempted; the Tauri app process and Vite dev server launched, but the command did not exit within the smoke-test timeout and produced no console output. Repo-specific Tauri/Vite processes were stopped afterward.

**Follow-ups**
- Repeat the full manual UI smoke test on the operator PC, including close -> restart -> open -> restart and Excel-open/locked workbook behavior.
- Future hardware integration should add a new source such as `Automated` without changing existing `Manual` log meanings.

## 2026-06-24 — Manual close logging workflow implemented

**What changed**
- Replaced the two-button On/Off UI with one PDU-style **Close Valve** workflow that starts with the valve shown as currently open.
- Added a PDU-style operator-name prompt with default saved names (`Sean`, `Long`, `Jose`), typed names, Enter-to-confirm, dropdown filtering, saved-name removal, blank-name validation, and case-insensitive dedupe.
- Added frontend Tauri wrappers for `log_valve_closed` and `open_valve_log`.
- Added Rust Tauri commands, durable JSONL close-event storage, Excel workbook regeneration, and workbook opening through the system default app.
- Added Rust dependencies for local timestamps, UUID event IDs, and XLSX generation.
- Updated `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`, and `docs/PROJECT_STRUCTURE.md` for the manual workflow and new files.

**Why**
- This is a temporary manual logging workflow until a later automated valve integration exists.
- JSONL is the durable source so close events are saved before Excel refresh, reducing data-loss risk when Excel has the workbook open or locked.

**How to verify**
- `bun run build`
- `cd backend && cargo check`
- `cd backend && cargo test`
- `bun run desktop` was attempted; the Tauri app process launched, but the command did not exit within the smoke-test timeout and produced no console output, so repo-specific Tauri/Vite processes were stopped afterward.

**Follow-ups**
- Manually verify the prompt and workbook on the operator machine: blank-name rejection, saved-name add/remove/filter behavior, close logging, **Open Log**, and Excel-lock retry behavior.
- When hardware integration is added, keep existing `Manual` log semantics and add a new source such as `Automated` for confirmed hardware-driven events.

## 2026-06-24 — Manual close logging implementation plan

**What changed**
- Added `docs/MANUAL_CLOSE_VALVE_LOGGING_PLAN.md` with the planned one-button manual close workflow, PDU-style operator prompt behavior, Excel log storage plan, backend/frontend file checklist, validation checklist, and future automated-valve handoff notes.

**Why**
- User confirmed this is a temporary manual close log until a later automated valve integration exists.
- The implementation needs a durable checklist so future work does not miss operator-name management, Excel logging, or the no-hardware-control caveat.

**How to verify**
- Open `docs/MANUAL_CLOSE_VALVE_LOGGING_PLAN.md` and confirm it covers UI behavior, backend commands, storage, validation, and documentation updates.

**Follow-ups**
- Implement the plan.
- After implementation, update `AGENTS.md`, `README.md`, `docs/ARCHITECTURE.md`, `docs/PROJECT_STRUCTURE.md`, and this changelog with the actual behavior and validation results.

## 2026-06-24 — Initial app shell and Bun migration

**What changed**
- Created Tauri 2 + React 19 + TypeScript + Vite + Tailwind v4 project from scratch.
- Built `ValvePanel` with two large **On** / **Off** buttons using PDU color palette.
- Migrated from npm to Bun (`bun.lock`, removed `package-lock.json`).
- Tauri scripts use `bun --cwd backend ../node_modules/@tauri-apps/cli/tauri.js` for Windows compatibility.
- Added `AGENTS.md`, `docs/PROJECT_STRUCTURE.md`, `docs/ARCHITECTURE.md`, and this changelog.

**Why**
- User wanted a nitrogen valve control app matching the PDU operator-panel design.
- Bun is the preferred package manager across `C:\Projects` workspaces.

**How to verify**
- `bun install`
- `bun run build`
- `bun run desktop` — window opens with On/Off buttons; status text updates on click.

**Follow-ups**
- Wire On/Off to real hardware via Rust Tauri commands.
- Add valve event logging with timestamps.
- Decide log storage format and path.
