export const knownGoodColorPalette = {
  shell: {
    background: "#20201f",
    panel: "#292928",
    input: "#1f1f1e",
  },
  states: {
    open: "#1d7f47",
    closed: "#343434",
  },
  controls: {
    browseReset: "#3a3a38",
    browseResetHover: "#454542",
  },
  ring: {
    current: "cyan-200/65",
    focus: "cyan-200/25",
  },
} as const;

export type ValveState = "open" | "closed";
export type ValveAction = "close" | "open";

export const actionButtonStyles: Record<ValveAction, string> = {
  close: "bg-[#d42c1a] text-white hover:bg-[#ca3c2d]",
  open: "bg-[#1d7f47] text-white hover:bg-[#1d7f46]",
};

export const idleButtonStyle = "bg-[#343434] text-white hover:bg-[#3d3d3d]";
