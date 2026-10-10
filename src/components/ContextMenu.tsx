/** One entry of a right-click menu. */
export interface MenuItem {
  label: string;
  /** Runs when the entry is clicked. The menu closes itself first. */
  onSelect: () => void;
  /** Shown greyed out and cannot be clicked. */
  disabled?: boolean;
}

interface ContextMenuProps {
  /** Where the pointer was when the menu was asked for (window coordinates). */
  x: number;
  y: number;
  items: MenuItem[];
  /** Called when the menu should go away: an entry was chosen, or the user
   *  clicked somewhere else. */
  onClose: () => void;
}

/** Roughly how big the menu is, to keep it inside the window. Must be at
 *  least what the classes on the menu below produce. */
const MENU_WIDTH = 230;
const ITEM_HEIGHT = 34;
const MENU_PADDING = 22;

/**
 * The app's own right-click menu, used wherever the webview's built-in one
 * would be wrong (its entries, such as "Save Image As" and "Reload", are
 * meant for web pages and mostly do nothing in a desktop app).
 *
 * The component only draws the menu. Whoever uses it decides when it is
 * open: typically by keeping `{ x, y }` in state, setting it in an
 * `onContextMenu` handler (after `event.preventDefault()`, which is what
 * suppresses the built-in menu) and clearing it in `onClose`.
 */
export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const height = items.length * ITEM_HEIGHT + MENU_PADDING;

  return (
    <>
      {/* An invisible full-window layer: clicking anywhere closes the menu.
          It and the menu sit above everything (`z-40`, `z-41`), the edit
          dialog included: a menu can be opened from the cover in there. */}
      <div
        className="fixed inset-0 z-40"
        onClick={(event) => {
          event.stopPropagation(); // do not also click whatever is underneath
          onClose();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          onClose();
        }}
      />
      <div
        className="fixed z-41 flex min-w-42.5 flex-col rounded-lg border border-edge bg-panel p-1 shadow-[0_8px_24px_rgb(0_0_0/45%)]"
        role="menu"
        // Keep the menu inside the window when opened near an edge.
        style={{
          left: Math.max(0, Math.min(x, window.innerWidth - MENU_WIDTH)),
          top: Math.max(0, Math.min(y, window.innerHeight - height)),
        }}
      >
        {items.map((item) => (
          <button
            key={item.label}
            role="menuitem"
            className="rounded-[5px] px-3 py-1.75 text-left enabled:hover:bg-raised"
            disabled={item.disabled}
            onClick={(event) => {
              event.stopPropagation();
              onClose();
              item.onSelect();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
    </>
  );
}
