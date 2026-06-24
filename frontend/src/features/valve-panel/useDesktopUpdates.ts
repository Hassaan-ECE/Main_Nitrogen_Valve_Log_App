import { useCallback, useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";

import { isTauriRuntime } from "@/integrations/tauri/runtime";

import { buildIdleUpdateState, type UpdateState } from "./updateTypes";

const UPDATE_CHECK_INTERVAL_MS = 5 * 60_000;
const INITIAL_UPDATE_CHECK_DELAY_MS = 1_500;

export function useDesktopUpdates(currentVersion: string) {
  const [updateState, setUpdateState] = useState<UpdateState>(() =>
    buildIdleUpdateState(currentVersion),
  );
  const currentVersionRef = useRef(currentVersion);
  const pendingUpdateRef = useRef<Update | null>(null);

  useEffect(() => {
    currentVersionRef.current = currentVersion;
    setUpdateState((current) =>
      current.currentVersion === currentVersion
        ? current
        : { ...current, currentVersion },
    );
  }, [currentVersion]);

  const checkForUpdate = useCallback(async (): Promise<UpdateState> => {
    const version = currentVersionRef.current;

    if (!isTauriRuntime()) {
      return buildIdleUpdateState(version);
    }

    setUpdateState({
      available: false,
      currentVersion: version,
      status: "checking",
    });

    try {
      const update = await check();
      pendingUpdateRef.current?.close().catch(() => undefined);
      pendingUpdateRef.current = update;

      if (!update) {
        const nextState: UpdateState = {
          available: false,
          currentVersion: version,
          status: "not-available",
        };
        setUpdateState(nextState);
        return nextState;
      }

      const nextState: UpdateState = {
        available: true,
        currentVersion: version,
        latestVersion: update.version,
        status: "available",
      };
      setUpdateState(nextState);
      return nextState;
    } catch (error) {
      pendingUpdateRef.current = null;
      const message =
        error instanceof Error ? error.message : "Update check failed.";
      const nextState: UpdateState = {
        available: false,
        currentVersion: version,
        error: message,
        status: "error",
      };
      setUpdateState(nextState);
      return nextState;
    }
  }, []);

  const installAvailableUpdate = useCallback(async (): Promise<void> => {
    const update = pendingUpdateRef.current;
    const version = currentVersionRef.current;

    if (!update || !isTauriRuntime()) {
      return;
    }

    setUpdateState({
      available: true,
      currentVersion: version,
      latestVersion: update.version,
      status: "downloading",
    });

    try {
      await update.downloadAndInstall();
      setUpdateState({
        available: false,
        currentVersion: version,
        latestVersion: update.version,
        status: "installing",
      });
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Update install failed.";
      setUpdateState({
        available: true,
        currentVersion: version,
        error: message,
        latestVersion: update.version,
        status: "error",
      });
    }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return undefined;
    }

    let active = true;
    const runCheck = (): void => {
      void checkForUpdate().then((state) => {
        if (active) {
          setUpdateState(state);
        }
      });
    };

    const initialTimeoutId = window.setTimeout(runCheck, INITIAL_UPDATE_CHECK_DELAY_MS);
    const intervalId = window.setInterval(runCheck, UPDATE_CHECK_INTERVAL_MS);

    return () => {
      active = false;
      window.clearTimeout(initialTimeoutId);
      window.clearInterval(intervalId);
      pendingUpdateRef.current?.close().catch(() => undefined);
      pendingUpdateRef.current = null;
    };
  }, [checkForUpdate]);

  return {
    installAvailableUpdate,
    updateState,
  };
}
