import { useEffect, useMemo, useState } from "react";

import {
  getAppStatus,
  getCurrentValveState,
  logValveClosed,
  logValveOpened,
  onValveLogChanged,
  openValveLog,
  openValveLogFolder,
  VALVE_STATE_POLL_INTERVAL_MS,
  valveLogErrorFromUnknown,
  type ValveLogEntry,
  type ValveState,
  type ValveStateSnapshot,
} from "@/integrations/tauri/valveLog";
import { cn } from "@/shared/lib/utils";

import {
  addOperatorName,
  loadOperatorNames,
  matchingOperatorNames,
  operatorNameKey,
  storeOperatorNames,
} from "./operatorNames";
import {
  actionButtonStyles,
  idleButtonStyle,
  type ValveAction,
} from "./stateStyles";
import {
  shouldShowValveUpdateButton,
  UpdateActionButton,
} from "./UpdateActionButton";
import { useDesktopUpdates } from "./useDesktopUpdates";

type PendingAction = ValveAction;

const actionConfig: Record<
  PendingAction,
  {
    title: string;
    buttonLabel: string;
    successState: ValveState;
  }
> = {
  close: {
    title: "Log Valve Closed",
    buttonLabel: "Close Valve",
    successState: "closed",
  },
  open: {
    title: "Log Valve Opened",
    buttonLabel: "Open Valve",
    successState: "open",
  },
};

type SharedSyncDisplay = "connected" | "local" | "not_connected";

function sharedSyncDisplayFromSnapshot(
  snapshot: ValveStateSnapshot,
): SharedSyncDisplay {
  if (!snapshot.shared_available) {
    return "not_connected";
  }

  if (snapshot.saved_locally_only) {
    return "local";
  }

  return "connected";
}

const sharedSyncLabels: Record<SharedSyncDisplay, string> = {
  connected: "Connected",
  local: "Local",
  not_connected: "Not Connected",
};

const sharedSyncColors: Record<SharedSyncDisplay, string> = {
  connected: "text-[#6fcf97]",
  local: "text-[#f4b1a9]",
  not_connected: "text-[#f4b1a9]",
};

function PrimaryValveButton({
  label,
  action,
  disabled,
  loading,
  onClick,
}: {
  label: string;
  action: ValveAction;
  disabled: boolean;
  loading?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "flex min-h-0 flex-1 items-center justify-center rounded-md px-6 py-8 text-center shadow-sm transition",
        "focus:outline-none focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-cyan-200/25 focus-visible:ring-offset-2 focus-visible:ring-offset-[#20201f]",
        !disabled &&
          "active:ring-2 active:ring-cyan-200/65 active:ring-offset-2 active:ring-offset-[#20201f]",
        disabled ? "cursor-not-allowed opacity-80" : actionButtonStyles[action],
        disabled && idleButtonStyle,
      )}
    >
      <span className="text-[27pt] font-bold leading-none tracking-wide">
        {loading ? "Loading..." : label}
      </span>
    </button>
  );
}

function stateFromEntry(
  entry: ValveLogEntry,
  fallback: ValveState,
): ValveState {
  const newState = entry.new_state.trim().toLowerCase();

  if (newState === "open" || newState === "closed") {
    return newState;
  }

  return fallback;
}

function latestLoggedStateSummary(snapshot: ValveStateSnapshot): string | null {
  if (snapshot.assumed) {
    return "Latest logged state: Open (assumed)";
  }

  if (snapshot.last_entry) {
    const state = snapshot.last_entry.new_state.trim().toLowerCase();
    const label = state === "closed" ? "Closed" : "Open";

    return `Latest logged state: ${label} by ${snapshot.last_entry.operator_name} at ${snapshot.last_entry.logged_at_local}`;
  }

  const label = snapshot.state === "closed" ? "Closed" : "Open";
  return `Latest logged state: ${label}`;
}

export function ValvePanel() {
  const [valveState, setValveState] = useState<ValveState>("open");
  const [isStateLoading, setIsStateLoading] = useState(true);
  const [operatorPromptOpen, setOperatorPromptOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<PendingAction>("close");
  const [operatorNames, setOperatorNames] =
    useState<string[]>(loadOperatorNames);
  const [operatorNameDraft, setOperatorNameDraft] = useState("");
  const [operatorNameError, setOperatorNameError] = useState("");
  const [operatorDropdownOpen, setOperatorDropdownOpen] = useState(false);
  const [operatorFilterText, setOperatorFilterText] = useState("");
  const [isLogging, setIsLogging] = useState(false);
  const [isOpeningExcelLog, setIsOpeningExcelLog] = useState(false);
  const [isOpeningLogFolder, setIsOpeningLogFolder] = useState(false);
  const [statusText, setStatusText] = useState<string | null>(null);
  const [sharedSyncDisplay, setSharedSyncDisplay] =
    useState<SharedSyncDisplay | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [appVersion, setAppVersion] = useState("0.1.7");
  const { installAvailableUpdate, updateState } = useDesktopUpdates(
    appVersion,
    !isStateLoading,
  );

  const visibleOperatorNames = useMemo(
    () => matchingOperatorNames(operatorNames, operatorFilterText),
    [operatorFilterText, operatorNames],
  );
  const currentAction: PendingAction = valveState === "open" ? "close" : "open";
  const currentActionConfig = actionConfig[currentAction];
  const promptConfig = actionConfig[pendingAction];

  function applySnapshot(
    snapshot: ValveStateSnapshot,
    showSharedWarning = false,
  ) {
    setValveState(snapshot.state);
    setStatusText(latestLoggedStateSummary(snapshot));
    setSharedSyncDisplay(sharedSyncDisplayFromSnapshot(snapshot));

    if (!showSharedWarning) {
      return;
    }

    if (snapshot.saved_locally_only || snapshot.shared_available === false) {
      setErrorText(
        snapshot.sync_message ||
          "Shared sync unavailable — event saved locally only.",
      );
    }
  }

  useEffect(() => {
    let cancelled = false;

    void getAppStatus()
      .then((status) => {
        if (!cancelled) {
          setAppVersion(status.version);
        }
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function loadCurrentState(options?: {
      initial?: boolean;
      showSharedWarning?: boolean;
    }) {
      if (options?.initial) {
        setIsStateLoading(true);
        setErrorText(null);
      }

      try {
        const snapshot = await getCurrentValveState();

        if (cancelled) {
          return;
        }

        applySnapshot(snapshot, options?.showSharedWarning);
      } catch (error) {
        if (cancelled) {
          return;
        }

        const valveLogError = valveLogErrorFromUnknown(error);
        setErrorText(valveLogError.message);
      } finally {
        if (!cancelled && options?.initial) {
          setIsStateLoading(false);
        }
      }
    }

    void loadCurrentState({ initial: true, showSharedWarning: true });

    const intervalId = window.setInterval(() => {
      void loadCurrentState();
    }, VALVE_STATE_POLL_INTERVAL_MS);
    const unsubscribe = onValveLogChanged(() => {
      void loadCurrentState();
    });

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
      unsubscribe();
    };
  }, []);

  function openOperatorPrompt() {
    if (isStateLoading) {
      return;
    }

    setPendingAction(currentAction);
    setOperatorNameDraft((current) => current.trim());
    setOperatorNameError("");
    setOperatorDropdownOpen(false);
    setOperatorFilterText("");
    setOperatorPromptOpen(true);
  }

  function closeOperatorPrompt() {
    setOperatorPromptOpen(false);
    setOperatorNameError("");
    setOperatorDropdownOpen(false);
    setOperatorFilterText("");
  }

  function handleRemoveOperatorName(name: string) {
    setOperatorNames((current) => {
      const key = operatorNameKey(name);
      const next = current.filter(
        (operatorName) => operatorNameKey(operatorName) !== key,
      );

      return storeOperatorNames(next);
    });

    if (operatorNameKey(operatorNameDraft) === operatorNameKey(name)) {
      setOperatorNameDraft("");
    }
  }

  function handleSelectOperatorName(name: string) {
    setOperatorNameDraft(name);
    setOperatorNameError("");
    setOperatorDropdownOpen(false);
    setOperatorFilterText("");
  }

  function applySuccessfulLog(
    entry: ValveLogEntry,
    action: PendingAction,
    warning?: string,
  ) {
    const nextState = stateFromEntry(entry, actionConfig[action].successState);

    setOperatorNames((current) =>
      storeOperatorNames(addOperatorName(current, entry.operator_name)),
    );
    setValveState(nextState);
    setStatusText(
      `Latest logged state: ${nextState === "closed" ? "Closed" : "Open"} by ${entry.operator_name} at ${entry.logged_at_local}`,
    );
    setErrorText(warning ?? null);
    closeOperatorPrompt();
  }

  async function handleConfirmLog() {
    const operatorName = operatorNameDraft.trim();

    if (!operatorName) {
      setOperatorNameError("Operator name is required.");
      return;
    }

    setIsLogging(true);
    setOperatorNameError("");
    setErrorText(null);

    try {
      const entry =
        pendingAction === "close"
          ? await logValveClosed(operatorName)
          : await logValveOpened(operatorName);
      applySuccessfulLog(entry, pendingAction);
    } catch (error) {
      const valveLogError = valveLogErrorFromUnknown(error);

      if (valveLogError.event_saved && valveLogError.entry) {
        applySuccessfulLog(
          valveLogError.entry,
          pendingAction,
          `${valveLogError.message} The event is saved in the durable source log.`,
        );
        return;
      }

      const errorMessage = valveLogError.detail
        ? `${valveLogError.message} ${valveLogError.detail}`
        : valveLogError.message;
      setOperatorNameError(errorMessage);
      setErrorText(errorMessage);
    } finally {
      setIsLogging(false);
    }
  }

  async function handleOpenExcelLog() {
    setIsOpeningExcelLog(true);
    setErrorText(null);

    try {
      await openValveLog();
    } catch (error) {
      const valveLogError = valveLogErrorFromUnknown(error);
      setErrorText(valveLogError.message);
    } finally {
      setIsOpeningExcelLog(false);
    }
  }

  async function handleOpenLogFolder() {
    setIsOpeningLogFolder(true);
    setErrorText(null);

    try {
      await openValveLogFolder();
    } catch (error) {
      const valveLogError = valveLogErrorFromUnknown(error);
      setErrorText(valveLogError.message);
    } finally {
      setIsOpeningLogFolder(false);
    }
  }

  return (
    <main className="flex h-screen min-h-[400px] w-full min-w-[360px] max-w-full flex-col overflow-hidden bg-[#20201f] p-3.5 text-white">
      <section className="flex min-h-0 flex-1 flex-col gap-3 px-1 pt-1">
        <PrimaryValveButton
          label={
            isStateLoading ? "Loading..." : currentActionConfig.buttonLabel
          }
          action={currentAction}
          disabled={isStateLoading || isLogging}
          loading={isStateLoading}
          onClick={openOperatorPrompt}
        />

        {statusText ? (
          <p className="text-center text-[8.5pt] leading-tight text-[#d8d2c8]">
            {statusText}
          </p>
        ) : null}

        {errorText ? (
          <p
            role="alert"
            className="text-center text-[8pt] leading-tight text-[#f4b1a9]"
          >
            {errorText}
          </p>
        ) : null}

        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            onClick={() => void handleOpenExcelLog()}
            disabled={isOpeningExcelLog || isOpeningLogFolder || isLogging}
            className="inline-flex min-h-10 items-center justify-center rounded-md bg-[#3a3a38] px-2 py-2 text-[8.5pt] font-semibold text-white shadow-sm transition hover:bg-[#454542] focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-200/25 focus-visible:ring-offset-2 focus-visible:ring-offset-[#20201f] disabled:cursor-not-allowed disabled:opacity-65"
          >
            {isOpeningExcelLog ? "Opening..." : "Open Excel Log"}
          </button>
          <button
            type="button"
            onClick={() => void handleOpenLogFolder()}
            disabled={isOpeningExcelLog || isOpeningLogFolder || isLogging}
            className="inline-flex min-h-10 items-center justify-center rounded-md bg-[#3a3a38] px-2 py-2 text-[8.5pt] font-semibold text-white shadow-sm transition hover:bg-[#454542] focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-200/25 focus-visible:ring-offset-2 focus-visible:ring-offset-[#20201f] disabled:cursor-not-allowed disabled:opacity-65"
          >
            {isOpeningLogFolder ? "Opening..." : "Open Log Folder"}
          </button>
        </div>
      </section>

      <footer className="mt-2 border-t border-[#454542] pt-2 text-[7.5pt] leading-tight text-[#d8d2c8]">
        <div className="flex items-center justify-between gap-3">
          <span className="flex min-w-0 items-center">
            <span className="truncate">
              v{appVersion}
              {sharedSyncDisplay ? (
                <>
                  {" "}
                  -{" "}
                  <span
                    className={cn(
                      "font-medium",
                      sharedSyncColors[sharedSyncDisplay],
                    )}
                  >
                    {sharedSyncLabels[sharedSyncDisplay]}
                  </span>
                </>
              ) : null}
            </span>
            {shouldShowValveUpdateButton(updateState) ? (
              <span className="ml-2 shrink-0">
                <UpdateActionButton
                  state={updateState}
                  onClick={() => void installAvailableUpdate()}
                />
              </span>
            ) : null}
          </span>
          <span className="shrink-0 font-medium">
            Built by Syed Hassaan Shah
          </span>
        </div>
      </footer>

      {operatorPromptOpen ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4">
          <section className="w-full max-w-[320px] rounded-md border border-[#454542] bg-[#292928] p-4 text-white shadow-2xl">
            <div className="text-center text-[12pt] font-semibold leading-tight">
              {promptConfig.title}
            </div>
            <div className="mt-1 text-center text-[8pt] leading-tight text-[#d8d2c8]">
              Enter the operator name to record a manual valve log.
            </div>
            <div className="relative mt-4">
              <div
                className={cn(
                  "flex h-9 rounded border bg-[#1f1f1e] focus-within:ring-2 focus-within:ring-cyan-200/25",
                  operatorNameError
                    ? "border-[#d42c1a]"
                    : "border-[#454542] focus-within:border-[#1f74ae]",
                )}
              >
                <input
                  aria-controls="valve-log-operator-menu"
                  aria-expanded={operatorDropdownOpen}
                  aria-haspopup="listbox"
                  aria-label="Operator name"
                  autoFocus
                  value={operatorNameDraft}
                  placeholder="Operator name..."
                  onChange={(event) => {
                    const value = event.target.value;

                    setOperatorNameDraft(value);
                    setOperatorFilterText(value);
                    setOperatorNameError("");
                    setOperatorDropdownOpen(true);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void handleConfirmLog();
                    } else if (event.key === "ArrowDown") {
                      event.preventDefault();
                      setOperatorFilterText(operatorNameDraft);
                      setOperatorDropdownOpen(true);
                    } else if (event.key === "Escape") {
                      setOperatorDropdownOpen(false);
                    }
                  }}
                  className="min-w-0 flex-1 rounded-l bg-transparent px-2 text-[9pt] text-white placeholder:text-[#b7b1a8] outline-none"
                />
                <button
                  type="button"
                  aria-label="Show operator names"
                  aria-expanded={operatorDropdownOpen}
                  aria-controls="valve-log-operator-menu"
                  onClick={() => {
                    setOperatorFilterText("");
                    setOperatorDropdownOpen((open) => !open);
                  }}
                  className="inline-flex w-9 shrink-0 items-center justify-center rounded-r border-l border-[#454542] text-[8.5pt] font-bold text-[#d8d2c8] transition hover:bg-[#353534] hover:text-white"
                >
                  v
                </button>
              </div>
              {operatorDropdownOpen ? (
                <div
                  id="valve-log-operator-menu"
                  role="listbox"
                  aria-label="Saved operators"
                  className="absolute left-0 right-0 top-full z-10 mt-1 max-h-36 overflow-y-auto rounded border border-[#454542] bg-[#242423] p-1 shadow-xl [scrollbar-width:thin]"
                >
                  {visibleOperatorNames.length ? (
                    visibleOperatorNames.map((name) => (
                      <div
                        key={name}
                        className="flex min-h-8 items-center gap-1 rounded hover:bg-[#30302f]"
                      >
                        <button
                          type="button"
                          role="option"
                          aria-selected={
                            operatorNameKey(operatorNameDraft) ===
                            operatorNameKey(name)
                          }
                          onClick={() => handleSelectOperatorName(name)}
                          className="min-w-0 flex-1 truncate px-2 text-left text-[8.5pt] font-medium text-white"
                        >
                          {name}
                        </button>
                        <button
                          type="button"
                          aria-label={`Remove ${name}`}
                          title={`Remove ${name}`}
                          onClick={() => handleRemoveOperatorName(name)}
                          className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded text-[10pt] font-bold text-[#d8d2c8] transition hover:bg-[#454542] hover:text-white"
                        >
                          x
                        </button>
                      </div>
                    ))
                  ) : (
                    <div className="px-2 py-1.5 text-[8pt] text-[#b7b1a8]">
                      {operatorNames.length
                        ? "No matching operators"
                        : "No saved operators"}
                    </div>
                  )}
                </div>
              ) : null}
              {operatorNameError ? (
                <div
                  role="alert"
                  className="mt-1.5 text-[7.5pt] leading-tight text-[#f4b1a9]"
                >
                  {operatorNameError}
                </div>
              ) : null}
            </div>
            <div className="mt-4 grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={closeOperatorPrompt}
                disabled={isLogging}
                className="inline-flex min-h-9 items-center justify-center rounded-md bg-[#3a3a38] px-3 py-2 text-[9pt] font-semibold text-white shadow-sm transition hover:bg-[#454542] disabled:cursor-not-allowed disabled:opacity-65"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void handleConfirmLog()}
                disabled={isLogging}
                className="inline-flex min-h-9 items-center justify-center rounded-md bg-[#1d7f47] px-3 py-2 text-[9pt] font-semibold text-white shadow-sm transition hover:bg-[#1d7f46] disabled:cursor-not-allowed disabled:opacity-65"
              >
                {isLogging ? "Confirming..." : "Confirm"}
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </main>
  );
}
