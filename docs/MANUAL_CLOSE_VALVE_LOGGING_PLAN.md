# Manual Close Valve Logging Plan

> Historical note: this close-only plan has been superseded by the implemented alternating manual open/close workflow. Current behavior is documented in `README.md` and `docs/ARCHITECTURE.md`.

This plan covers the temporary manual workflow for logging that the main nitrogen valve was closed before the operator leaves. It does not add hardware control or hardware state detection. The app should keep saying this is a manual log until a later automated valve integration exists.

## Goal

Replace the two-button local UI shell with a one-button operator workflow:

1. The app starts with the valve shown as currently open.
2. The operator clicks **Close Valve**.
3. The app asks for the operator name with the same saved-name pattern used by the PDU app's **Print Report** prompt.
4. The operator confirms.
5. The app writes a durable log entry and refreshes an Excel workbook that operators can open from the app.
6. The current app session shows the valve as closed and prevents a second accidental close log.

## Acceptance Criteria

- The main panel shows one large primary action button: **Close Valve**.
- Startup status reads as an assumed manual state, for example `Valve currently OPEN`.
- Clicking **Close Valve** opens an operator prompt before any log entry gets written.
- The operator prompt supports:
  - default saved names: `Sean`, `Long`, `Jose`
  - typing a new operator name
  - confirming with Enter
  - dropdown filtering
  - adding a typed name after a successful confirm
  - removing saved names from the dropdown
  - case-insensitive duplicate prevention
- Confirming with a blank operator name shows an inline error and writes nothing.
- A successful confirm logs:
  - local timestamp
  - valve name
  - action
  - previous state
  - new state
  - operator name
  - source, set to `Manual`
- After a successful log, the panel shows the valve as closed for the current app session.
- The app includes a small secondary **Open Log** control that opens the Excel log workbook.
- If the Excel workbook does not exist, **Open Log** creates it with headers before opening it.
- The implementation does not claim that the physical valve changed state.
- The PDU-style dark operator panel remains intact.

## Non-Goals

- Do not add PLC, serial, GPIO, or hardware control.
- Do not poll hardware state.
- Do not add settings screens, navigation, or a multi-valve workflow.
- Do not silently print or export reports.
- Do not make the Excel workbook the only durable source if that creates data-loss risk when Excel has the file open.

## UI Plan

Update `frontend/src/features/valve-panel/ValvePanel.tsx`.

Panel layout:

- Header: `Nitrogen Valve`
- Status text while open: `Valve currently OPEN`
- Status text after logging closed: `Valve CLOSED - logged by {operator}`
- Main button while open: **Close Valve**
- Main button after close: disabled **Valve Closed**
- Secondary footer or header button: **Open Log**

Modal layout should match the PDU prompt style:

- black translucent overlay
- `#292928` modal surface
- `#1f1f1e` input background
- `#454542` borders
- cyan focus ring
- green confirm button
- gray cancel button
- dropdown button with a chevron icon or a simple text-safe fallback if no icon library is added

Keep the window simple. The log opener is a utility action, not a second valve-state button.

## Operator Name Plan

Create `frontend/src/features/valve-panel/operatorNames.ts`.

Use the PDU app's logic as the source pattern:

- storage key: `nitrogenValve.operatorNames`
- `operatorNameKey(name)` trims and lowercases
- `normalizeOperatorNames(values)` removes blanks, non-strings, and duplicates
- `loadOperatorNames()` loads localStorage or seeds defaults
- `storeOperatorNames(names)` saves normalized names
- `addOperatorName(names, name)` appends a new normalized name
- `matchingOperatorNames(names, query)` returns starts-with matches first, then contains matches

Use localStorage only for the saved-name dropdown. The valve close events must go through the Rust backend.

## Frontend Integration Plan

Create `frontend/src/integrations/tauri/valveLog.ts`.

Expose typed wrappers:

```ts
export type ValveLogEntry = {
  id: string;
  logged_at_local: string;
  valve: string;
  action: string;
  previous_state: string;
  new_state: string;
  operator_name: string;
  source: string;
};

export async function logValveClosed(operatorName: string): Promise<ValveLogEntry>;
export async function openValveLog(): Promise<string>;
```

Expected invoke names:

- `log_valve_closed`
- `open_valve_log`

Frontend state to add:

- `valveState`: `open | closed`
- `operatorPromptOpen`
- `operatorNames`
- `operatorNameDraft`
- `operatorNameError`
- `operatorDropdownOpen`
- `operatorFilterText`
- `isLogging`
- `isOpeningLog`
- `lastLogEntry`
- `panelMessage`

Successful close flow:

1. Validate non-blank operator name in the frontend.
2. Call `logValveClosed(operatorName)`.
3. Store the operator name in localStorage.
4. Set `valveState` to `closed`.
5. Close the modal.
6. Show a concise success message with operator and timestamp.

Failure flow:

- Keep the modal open if logging fails.
- Show the backend error in the modal.
- Do not change `valveState` to `closed`.
- Do not add the typed operator name to saved names until the backend confirms the log write.

## Backend Plan

Add Rust command registration in `backend/src/lib.rs`.

Create these modules:

- `backend/src/commands.rs`
- `backend/src/valve_log.rs`

Commands:

```rust
#[tauri::command]
fn log_valve_closed(operator_name: String) -> Result<ValveLogEntry, ValveLogErrorDto>;

#[tauri::command]
fn open_valve_log() -> Result<String, ValveLogErrorDto>;
```

Backend responsibilities:

- trim and validate operator names
- create the log directory if needed
- append the durable source entry
- regenerate or update the Excel workbook
- open the Excel workbook with the system default app
- return structured errors for the frontend

## Storage Plan

Use an operator-facing Excel workbook plus a durable source log.

Recommended default location:

```text
%USERPROFILE%\Documents\Main Nitrogen Valve Log\Main Nitrogen Valve Log.xlsx
```

Recommended durable source file:

```text
%USERPROFILE%\Documents\Main Nitrogen Valve Log\Main Nitrogen Valve Log.jsonl
```

Reason:

- Excel files can be locked when an operator has the workbook open.
- A JSONL source lets the app save the close event first, then regenerate the workbook.
- If Excel is locked, the app can report that the event was saved but the workbook needs to be closed before refresh.

Workbook sheet:

- Sheet name: `Valve Log`

Workbook columns:

| Column | Header | Example |
| --- | --- | --- |
| A | Timestamp | `2026-06-24 16:42:10` |
| B | Valve | `Main Nitrogen Valve` |
| C | Action | `Close Valve` |
| D | Previous State | `Open` |
| E | New State | `Closed` |
| F | Operator | `Sean` |
| G | Source | `Manual` |
| H | Notes | `Temporary manual close log` |

Implementation preference:

- Add a Rust XLSX writer dependency such as `rust_xlsxwriter`.
- Use Tauri's path APIs for the Documents directory when practical.
- Avoid frontend filesystem writes.
- Use Rust unit tests for log serialization and workbook creation.

## Error Handling

Map backend failures to short operator-facing messages:

- `blank_operator_name`: `Operator name is required.`
- `log_directory_failed`: `The log folder could not be created.`
- `source_log_write_failed`: `The close event could not be saved.`
- `excel_refresh_failed`: `The event was saved, but the Excel log could not be refreshed. Close Excel and try opening the log again.`
- `open_log_failed`: `The Excel log could not be opened.`

If the durable source write succeeds but Excel refresh fails, do not lose the event. Return enough detail for the UI to tell the operator that the event was saved and the workbook refresh is pending.

## File Checklist

Expected frontend files:

- `frontend/src/features/valve-panel/ValvePanel.tsx`
- `frontend/src/features/valve-panel/operatorNames.ts`
- `frontend/src/features/valve-panel/stateStyles.ts` if a closed-state style is needed
- `frontend/src/integrations/tauri/valveLog.ts`

Expected backend files:

- `backend/src/lib.rs`
- `backend/src/commands.rs`
- `backend/src/valve_log.rs`
- `backend/Cargo.toml`
- `backend/Cargo.lock`

Expected docs:

- `docs/AGENT_CHANGELOG.md`
- `AGENTS.md` if project status changes after implementation
- `README.md` after the user-facing workflow and log location exist
- `docs/ARCHITECTURE.md` if the frontend-to-backend logging flow is added
- `docs/PROJECT_STRUCTURE.md` if new source files are added

## Implementation Steps

1. Add the operator-name helper under `frontend/src/features/valve-panel/`.
2. Add the Tauri integration wrapper under `frontend/src/integrations/tauri/`.
3. Replace the two-button `ValvePanel` UI with the single-button close workflow.
4. Add the operator prompt using the PDU prompt behavior and dark styling.
5. Add backend log models and validation in `valve_log.rs`.
6. Add JSONL append behavior before touching the Excel workbook.
7. Add Excel workbook generation from all source log entries.
8. Add the `open_valve_log` command.
9. Register commands in `lib.rs`.
10. Update docs after the feature works.
11. Run validation.

## Validation Checklist

Use Bun for project commands.

Required checks after implementation:

```bash
bun run build
```

Backend check after Rust changes:

```bash
cd backend
cargo check
```

Desktop smoke test:

```bash
bun run desktop
```

Manual verification:

1. Launch the app.
2. Confirm the status says the valve is open.
3. Click **Close Valve**.
4. Try confirming with a blank name; confirm no log row gets written.
5. Select an existing operator and confirm.
6. Confirm the UI changes to closed.
7. Click **Open Log**.
8. Confirm the Excel workbook opens and contains the close event row.
9. Type a new operator name in a second run and confirm it appears in the saved-name list.
10. Remove a saved operator and confirm it disappears from the dropdown.

## Documentation Updates After Implementation

Append a dated entry to `docs/AGENT_CHANGELOG.md` with:

- what changed
- why this is a temporary manual logging workflow
- how to verify with `bun run build`, `cargo check`, and `bun run desktop`
- any caveat around Excel being open or locked

Update `AGENTS.md`:

- change status from UI shell only to manual close logging
- note that hardware control is still not implemented
- add the new key files

Update `README.md`:

- describe the one-button close workflow
- list the Excel log path
- keep Bun commands unchanged

Update `docs/ARCHITECTURE.md`:

- show React invoking Rust commands
- explain JSONL source plus Excel workbook output
- state that valve state remains manually assumed

Update `docs/PROJECT_STRUCTURE.md`:

- list new frontend integration files
- list new backend command/log files

## Future Automated Valve Handoff

When the automated valve is added later, keep the operator log format stable if possible. Add new sources rather than changing old meanings:

- `Manual` for the temporary workflow
- `Automated` for confirmed hardware-driven close events
- `Hardware Poll` if future state polling records observations

The automated version should replace the manual assumed-open state with hardware state from the backend. The Excel workbook can keep the same columns and add extra notes or diagnostic columns only if operators need them.
