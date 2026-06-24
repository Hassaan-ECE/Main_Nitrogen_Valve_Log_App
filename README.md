# Main Nitrogen Valve Log App

Desktop app for manually logging main nitrogen valve open/close events. Built as a small PDU-style operator panel with one primary button that alternates between **Close Valve** and **Open Valve**.

> **AI agents:** read [`AGENTS.md`](AGENTS.md) first, then check [`docs/AGENT_CHANGELOG.md`](docs/AGENT_CHANGELOG.md) for recent changes from other sessions.

This project reuses the same stack and visual design as the [PDU Data Automation App](C:\Projects\Active\PDU_Data_Automation_App): dark industrial panel, bold buttons, and a narrow centered window suited for lab use.

## Status

`v0.1.0` — temporary manual open/close valve logging.

Implemented now:

- Tauri 2 desktop window
- React/TypeScript frontend with Tailwind CSS
- One large alternating **Close Valve** / **Open Valve** button
- Startup state loaded from the latest JSONL log entry; if no valid entry exists, the app assumes the valve is open
- PDU-style operator-name prompt with saved names, filtering, removal, blank-name validation, and case-insensitive dedupe
- Durable JSONL source log for open and close events
- Generated Excel workbook that can be opened from the app

Not implemented yet:

- Hardware, PLC, serial, GPIO, or valve-state detection
- Settings, polling, or real-device error handling
- Automated valve control

The current workflow is manual logging only. It records that an operator confirmed an open or close log entry; it does not control or read the physical valve. The displayed state is based on the latest saved manual log entry.

## Stack

| Layer | Technology |
|-------|------------|
| Desktop shell | Tauri 2 (Rust) |
| UI | React 19, TypeScript, Vite |
| Styling | Tailwind CSS v4 |
| Package manager | Bun 1.3.14 |

## Design

The UI follows the PDU app operator panel style:

- Shell background: `#20201f`
- Open/latest-open state button: `#1d7f47`
- Closed/latest-closed state button: `#343434`
- Segoe UI typography
- Cyan focus/active ring on controls
- Window size: 360×500 (min 360×400)

Main UI code lives in `frontend/src/features/valve-panel/ValvePanel.tsx`.

## Manual open/close workflow

1. The app reads the latest JSONL log entry on startup.
2. If no valid log entry exists, it shows `Valve currently OPEN` and **Close Valve**.
3. If the latest `new_state` is `Closed`, it shows `Valve currently CLOSED` and **Open Valve**.
4. If the latest `new_state` is `Open`, it shows `Valve currently OPEN` and **Close Valve**.
5. The operator clicks the displayed primary action.
6. The app prompts for an operator name.
7. Confirming with a blank name shows an inline error and writes nothing.
8. Confirming a valid name writes a durable event and refreshes the Excel log.
9. The UI switches to the next state and the primary button changes to the opposite action.

End-of-day close:

1. The app shows `Valve currently OPEN`.
2. The operator clicks **Close Valve**.
3. The app prompts for an operator name.
4. Confirming logs `previous_state = Open` and `new_state = Closed`.
5. The UI changes to `Valve currently CLOSED` and the button changes to **Open Valve**.

Next-morning open:

1. The app reads the latest saved log and shows `Valve currently CLOSED`.
2. The operator clicks **Open Valve**.
3. The app prompts for an operator name.
4. Confirming logs `previous_state = Closed` and `new_state = Open`.
5. The UI changes to `Valve currently OPEN` and the button changes to **Close Valve**.

**Open Log** creates or refreshes the workbook and opens it with the system default app.

Saved operator names are stored only in browser localStorage under `nitrogenValve.operatorNames`.

## Log files

Default location:

```text
%USERPROFILE%\Documents\Main Nitrogen Valve Log\
```

Files:

- `Main Nitrogen Valve Log.jsonl` - durable source of open/close events
- `Main Nitrogen Valve Log.xlsx` - operator-facing Excel workbook generated from the JSONL source

If Excel has the workbook open and locked, the app saves the JSONL event first and then reports that the workbook could not be refreshed. Close Excel and use **Open Log** again to regenerate/open the workbook.

## Project layout

```text
Main_Nitrogen_Valve_Log_App/
├── AGENTS.md          Agent onboarding and constraints
├── docs/              Architecture, structure, agent changelog
├── frontend/          React UI (Vite + Tailwind)
│   └── src/
│       ├── app/       App entry and global styles
│       ├── features/valve-panel/   Manual open/close workflow
│       └── integrations/tauri/     Frontend invoke wrappers
├── backend/           Tauri/Rust desktop shell and log commands
│   ├── src/           Rust entry, commands, and valve log module
│   └── tauri.conf.json
├── bun.lock           Bun lockfile (use Bun for all installs/scripts)
└── package.json
```

## Prerequisites

- Bun 1.3.14+
- Rust toolchain (for Tauri)
- Windows build tools (for the desktop app on Windows)

## Run locally

Install dependencies:

```bash
bun install
```

Start the desktop app:

```bash
bun run desktop
```

Frontend-only preview in the browser:

```bash
bun run dev
```

## Build

Build the frontend:

```bash
bun run build
```

Build the Windows installer:

```bash
bun run build:desktop
```

## Documentation

| Doc | Purpose |
|-----|---------|
| [`AGENTS.md`](AGENTS.md) | Rules and context for AI agents |
| [`docs/AGENT_CHANGELOG.md`](docs/AGENT_CHANGELOG.md) | Living log of changes across agent sessions |
| [`docs/PROJECT_STRUCTURE.md`](docs/PROJECT_STRUCTURE.md) | Directory map and where to edit |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Stack, design system, data flow |

## Next steps

Likely follow-on work for this app:

1. Define the real valve hardware/interface path
2. Replace the assumed manual state with confirmed hardware state
3. Add connection/status feedback for the hardware layer
4. Preserve the log columns while adding future automated sources such as `Automated`
