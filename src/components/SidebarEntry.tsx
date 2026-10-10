import { useState, type ReactNode } from "react";
import {
  cx,
  iconButtonSubtle,
  iconButtonSubtleActive,
  smallButton,
  smallButtonDanger,
} from "../classes";
import { DROP_TARGET_ATTRIBUTE, dropTargetValue, type DropTarget } from "../hooks/useTrackDrag";
import { ChecklistIcon, CloseIcon } from "./Icons";

/**
 * How an entry looks while songs are being dragged from the list:
 * - "none":   nothing is being dragged; look normal.
 * - "dimmed": songs cannot be dropped here; fade back.
 * - "ready":  songs can be dropped here; dashed outline.
 * - "hover":  the pointer is over it; releasing now will drop.
 * - "refused" / "refused-hover": this is somewhere songs go, but it already
 *   has every song being dragged, so dropping would add nothing. Shown in
 *   the danger colour instead of the accent colour.
 */
export type DropState = "none" | "dimmed" | "ready" | "hover" | "refused" | "refused-hover";

interface SidebarEntryProps {
  name: string;
  /** A small icon shown before the name. */
  icon?: ReactNode;
  /** The small line under the name (usually the song count). */
  detail: string;
  /** Extra text at the right-hand end of the detail line (a size). */
  trailing?: string;
  /** Tooltip for the whole entry. */
  title?: string;
  /** Whether this is the entry whose songs are being shown. */
  selected: boolean;
  onSelect: () => void;
  drop: DropState;
  /** What dropping songs here means. Needed unless `drop` is "none" or "dimmed". */
  dropTarget?: DropTarget;
  /**
   * Gives the entry a "compare" button, which switches compare mode on or
   * off for it (the song list then marks which songs this entry has).
   */
  compare?: { active: boolean; title: string; onToggle: () => void };
  /**
   * Gives the entry an "x" button. Clicking it asks for confirmation in
   * place (showing `question` under the name) before calling `onConfirm`.
   */
  remove?: { title: string; question: string; onConfirm: () => void };
}

/**
 * One line of the sidebar: "All songs", a folder or a playlist. Clicking it
 * selects it; some can be removed; some can have songs dropped on them.
 *
 * The drag itself is run by `useTrackDrag`. This component only marks
 * itself as a target (with the `data-drop-target` attribute) and shows the
 * feedback.
 */
export function SidebarEntry({
  name,
  icon,
  detail,
  trailing,
  title,
  selected,
  onSelect,
  drop,
  dropTarget,
  compare,
  remove,
}: SidebarEntryProps) {
  // Removing takes two clicks; this is true between the first and second.
  const [confirming, setConfirming] = useState(false);

  // Whether the entry is somewhere songs go at all, and whether it is
  // turning these particular songs away.
  const refuses = drop === "refused" || drop === "refused-hover";
  const acceptsDrop = drop === "ready" || drop === "hover";
  const isTarget = acceptsDrop || refuses;

  return (
    <li
      className={cx(
        "flex items-center gap-1.5 rounded-md py-1.5 pr-1 pl-2",
        // The selected entry is raised, with an accent stripe down its
        // left edge (an inset shadow, so it takes up no space).
        selected && "bg-raised shadow-[inset_3px_0_0_var(--color-accent)]",
        !isTarget && "hover:bg-raised",
        // `-outline-offset-2` draws the outline inside the entry's edge,
        // where the list's scrolling box cannot clip it.
        isTarget && "outline-2 -outline-offset-2",
        acceptsDrop && "outline-accent",
        drop === "ready" && "bg-accent/10 outline-dashed",
        drop === "hover" && "bg-accent/28 outline-solid",
        refuses && "outline-danger",
        drop === "refused" && "bg-danger/10 outline-dashed",
        drop === "refused-hover" && "bg-danger/28 outline-solid",
        drop === "dimmed" && "opacity-40",
      )}
      title={title}
      // This attribute is how `useTrackDrag` recognises a place where
      // songs can be dropped. Spreading an empty object adds nothing, so
      // only entries songs can go to get it. That includes one that is
      // refusing them: the drag needs to know the pointer is over it, to
      // turn its own label red too.
      {...(isTarget && dropTarget
        ? { [DROP_TARGET_ATTRIBUTE]: dropTargetValue(dropTarget) }
        : {})}
    >
      {/* The name and detail are one big button that selects the entry; the
          remove button sits beside it, not inside it. `min-w-0` lets the
          button shrink, so a long name is cut short instead of pushing the
          row wider. */}
      <button className="min-w-0 flex-1 text-left" onClick={onSelect}>
        <div className="flex items-center gap-1.25 font-semibold">
          {/* The icon never shrinks. */}
          {icon && <span className="flex-none text-accent">{icon}</span>}
          <span className="truncate">{name}</span>
        </div>
        {/* Detail on the left, trailing text pushed to the right. The
            detail is cut short rather than wrapped, so every entry is the
            same height. The exception is the remove question, which gets
            the whole line and may wrap: it has to be read in full. */}
        <div
          className={cx(
            "flex justify-between gap-2 text-xs",
            acceptsDrop ? "text-accent" : refuses ? "text-danger" : "text-muted",
          )}
        >
          {confirming && remove ? (
            <span>{remove.question}</span>
          ) : (
            <span className="min-w-0 truncate">{detail}</span>
          )}
          {trailing && !confirming && (
            <span className="flex-none whitespace-nowrap">{trailing}</span>
          )}
        </div>
      </button>

      {compare && !confirming && (
        <button
          className={compare.active ? iconButtonSubtleActive : iconButtonSubtle}
          title={compare.title}
          // `aria-pressed` marks this as an on/off button for screen readers.
          aria-pressed={compare.active}
          onClick={compare.onToggle}
        >
          <ChecklistIcon size={16} />
        </button>
      )}

      {remove &&
        (confirming ? (
          <div className="flex flex-col gap-1">
            <button
              className={smallButtonDanger}
              onClick={() => {
                setConfirming(false);
                remove.onConfirm();
              }}
            >
              Remove
            </button>
            <button className={smallButton} onClick={() => setConfirming(false)}>
              Keep
            </button>
          </div>
        ) : (
          <button className={iconButtonSubtle} title={remove.title} onClick={() => setConfirming(true)}>
            <CloseIcon size={16} />
          </button>
        ))}
    </li>
  );
}
