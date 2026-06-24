# Project Structure

```text
Main_Nitrogen_Valve_Log_App/
├── AGENTS.md                 # Start here (AI agents + contributors)
├── README.md                 # User-facing overview and quick start
├── package.json              # Bun scripts and JS dependencies
├── bun.lock                  # Bun lockfile — commit this, use Bun only
├── tsconfig.json             # TS project references
│
├── frontend/                 # React UI (Vite root)
│   ├── index.html
│   ├── vite.config.ts
│   ├── tsconfig.app.json
│   ├── tsconfig.node.json
│   └── src/
│       ├── app/
│       │   ├── main.tsx      # React entry
│       │   ├── App.tsx       # Renders ValvePanel
│       │   └── index.css     # Global styles + Tailwind import
│       ├── features/
│       │   └── valve-panel/
│       │       ├── ValvePanel.tsx   # Main manual open/close valve UI
│       │       ├── operatorNames.ts # Saved operator-name helpers
│       │       └── stateStyles.ts   # Color palette + button states
│       ├── integrations/
│       │   └── tauri/
│       │       └── valveLog.ts      # state, open/close log, and Open Log wrappers
│       └── shared/
│           └── lib/
│               └── utils.ts         # cn() helper (clsx + tailwind-merge)
│
├── backend/                  # Tauri / Rust shell
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── build.rs
│   ├── tauri.conf.json       # Window config, build hooks, bundle settings
│   ├── capabilities/
│   │   └── default.json      # Tauri permissions
│   ├── icons/
│   │   └── icon.ico          # Copied from PDU app (placeholder)
│   └── src/
│       ├── main.rs           # Windows subsystem entry
│       ├── lib.rs            # Tauri builder + command registration
│       ├── commands.rs       # Tauri command handlers
│       └── valve_log.rs      # JSONL state source, transition checks, Excel handling
│
└── docs/
    ├── AGENT_CHANGELOG.md    # Cross-agent handoff log — append on changes
    ├── ARCHITECTURE.md       # Stack, design system, data flow
    ├── MANUAL_CLOSE_VALVE_LOGGING_PLAN.md  # Historical close-only plan, superseded by open/close workflow
    └── PROJECT_STRUCTURE.md  # This file
```

## Where To Make Common Changes

| Task | Where to edit |
|------|----------------|
| Manual open/close UI or operator prompt | `frontend/src/features/valve-panel/ValvePanel.tsx` |
| Saved operator-name behavior | `frontend/src/features/valve-panel/operatorNames.ts` |
| Colors / button states | `frontend/src/features/valve-panel/stateStyles.ts` |
| Window title, size, installer | `backend/tauri.conf.json` |
| Tauri command registration | `backend/src/lib.rs`, `backend/src/commands.rs` |
| Manual state lookup, transition checks, log storage, Excel generation | `backend/src/valve_log.rs` |
| Frontend -> Rust bridge | `frontend/src/integrations/tauri/valveLog.ts` |
| Bun / build scripts | `package.json`, `backend/tauri.conf.json` `build.*` hooks |
| Agent handoff notes | `docs/AGENT_CHANGELOG.md` |
| Manual close logging plan | `docs/MANUAL_CLOSE_VALVE_LOGGING_PLAN.md` |

## Generated / Ignored Paths

Not committed (see `.gitignore`):

- `node_modules/`
- `frontend/dist/`
- `backend/target/`

## Planned Additions (not created yet)

When the app grows, expect:

```text
config/                       # Hardware profiles, log paths
docs/decisions/               # Architecture decision records
```
