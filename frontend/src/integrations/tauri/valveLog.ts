import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ValveState = "open" | "closed";

export type ValveLogEntry = {
  id: string;
  logged_at_local: string;
  logged_at_utc?: string | null;
  timezone?: string | null;
  valve: string;
  action: string;
  previous_state: string;
  new_state: string;
  operator_name: string;
  source: string;
  notes?: string | null;
};

export type ValveStateSnapshot = {
  state: ValveState;
  assumed: boolean;
  last_entry: ValveLogEntry | null;
  shared_available?: boolean;
  saved_locally_only?: boolean;
  shared_sync_status?: string;
  last_shared_update?: string | null;
  sync_message?: string;
};

export type AppStatus = {
  app_name: string;
  version: string;
};

export const VALVE_LOG_CHANGED_EVENT = "valve-log:changed";
export const VALVE_STATE_POLL_INTERVAL_MS = 500;

export type ValveLogCommandError = {
  code: string;
  message: string;
  detail?: string | null;
  event_saved?: boolean;
  entry?: ValveLogEntry | null;
};

export async function getAppStatus(): Promise<AppStatus> {
  return invoke<AppStatus>("get_app_status");
}

export async function getCurrentValveState(): Promise<ValveStateSnapshot> {
  return invoke<ValveStateSnapshot>("get_current_valve_state");
}

export async function logValveClosed(
  operatorName: string,
): Promise<ValveLogEntry> {
  return invoke<ValveLogEntry>("log_valve_closed", { operatorName });
}

export async function logValveOpened(
  operatorName: string,
): Promise<ValveLogEntry> {
  return invoke<ValveLogEntry>("log_valve_opened", { operatorName });
}

export async function openValveLog(): Promise<string> {
  return invoke<string>("open_valve_log");
}

export async function openValveLogFolder(): Promise<string> {
  return invoke<string>("open_valve_log_folder");
}

export function onValveLogChanged(callback: () => void): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;

  void listen(VALVE_LOG_CHANGED_EVENT, () => {
    callback();
  })
    .then((stopListening) => {
      if (disposed) {
        stopListening();
        return;
      }

      unlisten = stopListening;
    })
    .catch(() => undefined);

  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}

export function valveLogErrorFromUnknown(error: unknown): ValveLogCommandError {
  if (typeof error === "object" && error !== null && "message" in error) {
    const candidate = error as Partial<ValveLogCommandError>;

    return {
      code: typeof candidate.code === "string" ? candidate.code : "unknown",
      message:
        typeof candidate.message === "string"
          ? candidate.message
          : "Valve log action failed.",
      detail: typeof candidate.detail === "string" ? candidate.detail : null,
      event_saved: candidate.event_saved === true,
      entry: candidate.entry ?? null,
    };
  }

  if (typeof error === "string") {
    return {
      code: "unknown",
      message: error,
      event_saved: false,
      entry: null,
    };
  }

  return {
    code: "unknown",
    message: "Valve log action failed.",
    event_saved: false,
    entry: null,
  };
}
