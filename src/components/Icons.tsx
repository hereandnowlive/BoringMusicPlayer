// The app's icons, drawn as inline SVG so no icon library is needed.
// Each one inherits its colour from the surrounding text (`currentColor`).

import type { ReactNode } from "react";

/** Shared wrapper: a 24x24 drawing area scaled to `size` pixels. */
function Icon({ size = 20, children }: { size?: number; children: ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      {children}
    </svg>
  );
}

type IconProps = { size?: number };

export const PlayIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M8 5v14l11-7z" />
  </Icon>
);

export const PauseIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M6 5h4v14H6zM14 5h4v14h-4z" />
  </Icon>
);

export const StopIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M6 6h12v12H6z" />
  </Icon>
);

export const NextIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M6 6v12l8.5-6zM16 6h2v12h-2z" />
  </Icon>
);

export const PreviousIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M18 6v12l-8.5-6zM6 6h2v12H6z" />
  </Icon>
);

export const ShuffleIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M10.59 9.17 5.41 4 4 5.41l5.17 5.17zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04z" />
  </Icon>
);

export const VolumeIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M3 9v6h4l5 5V4L7 9zm13.5 3A4.5 4.5 0 0 0 14 7.97v8.05c1.48-.73 2.5-2.25 2.5-4.02z" />
  </Icon>
);

export const NoteIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M12 3v10.55A4 4 0 1 0 14 17V7h4V3z" />
  </Icon>
);

export const CloseIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
  </Icon>
);

export const PlusIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6z" />
  </Icon>
);

/** A tick. */
export const CheckIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
  </Icon>
);

/** A ticked list: "compare the songs with this". */
export const ChecklistIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M22 7h-9v2h9zm0 8h-9v2h9zM5.54 11 2 7.46l1.41-1.41 2.12 2.12 4.24-4.24 1.41 1.41zm0 8L2 15.46l1.41-1.41 2.12 2.12 4.24-4.24 1.41 1.41z" />
  </Icon>
);

/** A circular arrow: "do it again" (rescan). */
export const RefreshIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M17.65 6.35A7.96 7.96 0 0 0 12 4a8 8 0 1 0 7.73 10h-2.08A6 6 0 1 1 12 6c1.66 0 3.14.69 4.22 1.78L13 11h7V4z" />
  </Icon>
);

/** A "v" arrowhead pointing down. Rotate it with CSS to point elsewhere. */
export const ChevronIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M7.41 8.59 12 13.17l4.59-4.58L18 10l-6 6-6-6z" />
  </Icon>
);

/** A portable music player. */
export const DeviceIcon = ({ size }: IconProps) => (
  <Icon size={size}>
    <path d="M16 2H8a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2zm-8 2h8v7H8zm4 15.5a3 3 0 1 1 0-6 3 3 0 0 1 0 6zm0-2a1 1 0 1 0 0-2 1 1 0 0 0 0 2z" />
  </Icon>
);
