import { cn } from "@/shared/lib/utils";

import type { UpdateState } from "./updateTypes";

interface UpdateActionButtonProps {
  onClick: () => void;
  state: UpdateState;
}

function InstallIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.25"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="M12 4v10" />
      <path d="M8 10l4 4 4-4" />
      <path d="M5 20h14" />
    </svg>
  );
}

function getUpdateActionLabel(state: UpdateState): string {
  switch (state.status) {
    case "downloading":
      return "Downloading";
    case "installing":
      return "Installing";
    case "error":
      return "Retry";
    default:
      return "Update";
  }
}

function shouldShowUpdateButton(state: UpdateState): boolean {
  return (
    state.status === "available" ||
    state.status === "downloading" ||
    state.status === "installing" ||
    state.status === "error"
  );
}

export function shouldShowValveUpdateButton(state: UpdateState): boolean {
  return shouldShowUpdateButton(state);
}

export function UpdateActionButton({
  onClick,
  state,
}: UpdateActionButtonProps) {
  if (!shouldShowUpdateButton(state)) {
    return null;
  }

  const label = getUpdateActionLabel(state);
  const isBusy =
    state.status === "downloading" || state.status === "installing";

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={isBusy}
      aria-label={
        state.latestVersion
          ? `${label} to version ${state.latestVersion}`
          : label
      }
      className={cn(
        "inline-flex h-4 shrink-0 items-center justify-center gap-1 rounded-sm bg-[#1f74ae] px-1.5 text-[7pt] font-semibold leading-none text-white transition",
        "hover:bg-[#2288c9] focus:outline-none focus-visible:ring-1 focus-visible:ring-cyan-200/40",
        isBusy && "cursor-wait opacity-90 hover:bg-[#1f74ae]",
      )}
    >
      {!isBusy ? <InstallIcon className="h-2.5 w-2.5 shrink-0" /> : null}
      <span>{label}</span>
    </button>
  );
}
