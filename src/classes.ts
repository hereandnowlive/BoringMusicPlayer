// Tailwind class strings shared by more than one component.
//
// With Tailwind, an element's styling is the list of utility classes in its
// `className`. When several elements should look the same (every text
// button, every round icon button), the list is written once here and
// imported, instead of being copied around.
//
// Each constant is a complete look. They are not meant to be combined with
// each other: two utilities that set the same property (say `border-edge`
// and `border-danger`) do not override one another in the order written,
// so a variant is its own constant built from the shared base.
//
// Tailwind finds the classes it must generate by reading the source files
// as plain text, so every class has to appear here in full. Do not build
// names from pieces (`"bg-" + colour`); that class would never be made.
// https://tailwindcss.com/docs/detecting-classes-in-source-files

/**
 * Joins class names into one string, skipping anything false or empty.
 * This is what makes conditional classes readable:
 *
 *     cx("flex gap-2", selected && "bg-raised", muted ? "text-muted" : "text-fg")
 */
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

// ------------------------------------------------------------------- buttons

const buttonBase = "rounded-md border px-3.5 py-1.75 whitespace-nowrap";

/** A normal text button. */
export const button = `${buttonBase} border-edge bg-raised enabled:hover:border-muted`;

/** The main action of a group, in the accent colour. */
export const buttonPrimary = `${buttonBase} border-accent bg-accent font-semibold text-on-accent enabled:hover:brightness-108`;

/** A text button for something destructive. */
export const buttonDanger = `${buttonBase} border-danger bg-raised text-danger enabled:hover:border-muted`;

const smallButtonBase = "rounded-[5px] border bg-raised px-2 py-0.75 text-xs";

/** A compact text button (the sidebar's Remove / Keep). */
export const smallButton = `${smallButtonBase} border-edge`;
export const smallButtonDanger = `${smallButtonBase} border-danger text-danger`;

const iconButtonBase = "grid flex-none place-items-center rounded-full";

/** A round button that holds only an icon. */
export const iconButton = `${iconButtonBase} size-9 text-fg enabled:hover:bg-raised`;

/** An icon button that is switched on (shuffle). */
export const iconButtonActive = `${iconButtonBase} size-9 text-accent enabled:hover:bg-raised`;

/** A smaller, quieter icon button (the little "x" buttons). */
export const iconButtonSubtle = `${iconButtonBase} size-6.5 text-muted enabled:hover:bg-raised`;

/** A small icon button that is switched on (compare mode). */
export const iconButtonSubtleActive = `${iconButtonBase} size-6.5 bg-raised text-accent`;

/** The big light-coloured play / pause button. */
export const playButton = `${iconButtonBase} size-10.5 bg-fg text-window enabled:hover:bg-white`;
