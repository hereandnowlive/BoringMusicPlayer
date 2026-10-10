import type { ReactNode } from "react";
import { cx } from "../classes";
import { ChevronIcon } from "./Icons";

interface SidebarSectionProps {
  title: string;
  /** Whether the section's contents are showing. */
  open: boolean;
  onToggle: () => void;
  /** Buttons for the right-hand end of the header (a "+", for example). */
  action?: ReactNode;
  /** Extra classes for the section itself (where it sits in the sidebar). */
  className?: string;
  children: ReactNode;
}

/**
 * A collapsible part of the sidebar (an "accordion" section): a header that
 * is always visible, and contents that the header shows or hides.
 *
 * While open, a section is at most half as tall as the sidebar
 * (`max-h-1/2`). Its contents are expected to include a list that scrolls
 * (a `ScrollList`), which is what gives way once that limit is
 * reached; `min-h-0` here is what lets the section be shorter than its
 * contents in the first place.
 * https://developer.mozilla.org/docs/Web/CSS/min-height#auto
 */
export function SidebarSection({
  title,
  open,
  onToggle,
  action,
  className,
  children,
}: SidebarSectionProps) {
  return (
    <section className={cx("flex min-h-0 flex-col gap-1.5", open && "max-h-1/2", className)}>
      <div className="flex flex-none items-center justify-between gap-2">
        {/* `aria-expanded` tells screen readers whether the section is
            open. https://developer.mozilla.org/docs/Web/Accessibility/ARIA/Attributes/aria-expanded */}
        <button
          className="flex min-w-0 flex-1 items-center gap-1 py-0.5 text-xs font-semibold tracking-[0.06em] text-muted uppercase hover:text-fg"
          aria-expanded={open}
          onClick={onToggle}
        >
          {/* Points down when open, right when closed. */}
          <span className={cx("flex-none transition-transform", !open && "-rotate-90")}>
            <ChevronIcon size={16} />
          </span>
          {title}
        </button>
        {action && <div className="flex flex-none items-center">{action}</div>}
      </div>

      {open && children}
    </section>
  );
}
