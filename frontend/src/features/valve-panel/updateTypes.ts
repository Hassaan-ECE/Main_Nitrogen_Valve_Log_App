export type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "not-available"
  | "downloading"
  | "installing"
  | "error";

export interface UpdateState {
  available: boolean;
  currentVersion: string;
  error?: string;
  latestVersion?: string;
  status: UpdateStatus;
}

export function buildIdleUpdateState(currentVersion: string): UpdateState {
  return {
    available: false,
    currentVersion,
    status: "idle",
  };
}
