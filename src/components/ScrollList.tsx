import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { cx } from "../classes";

/**
 * A list that scrolls when its items do not fit, and, while it is
 * scrolling, keeps its items clear of the scrollbar.
 *
 * The scrollbar is drawn along the list's right-hand edge, in some webviews
 * on top of the items. Anything at the right-hand end of an item (the
 * sidebar's "x" buttons) then sits under or right against it. So when the
 * list has more than fits, it gets some padding on that side; when
 * everything fits there is no scrollbar, and no padding either.
 *
 * CSS cannot ask "is this element overflowing?", so it is measured: an
 * element overflows when its content (`scrollHeight`) is taller than the
 * box it is shown in (`clientHeight`).
 * https://developer.mozilla.org/docs/Web/API/Element/scrollHeight
 */
export function ScrollList({ children }: { children: ReactNode }) {
  const listRef = useRef<HTMLUListElement>(null);
  const [overflowing, setOverflowing] = useState(false);

  // Measure after every render, because a render is how the items change.
  // `useLayoutEffect` runs before the browser paints, so the padding is in
  // place on the first frame instead of popping in a moment later. With no
  // dependency list it runs every time; setting state to the value it
  // already has does not cause another render, so this does not loop.
  // Docs: https://react.dev/reference/react/useLayoutEffect
  useLayoutEffect(() => {
    const list = listRef.current;
    if (list) setOverflowing(list.scrollHeight > list.clientHeight);
  });

  // The other way the answer changes is the list's own box changing size
  // (the window is resized, the other section is folded), which involves
  // no render here. A ResizeObserver reports those.
  // https://developer.mozilla.org/docs/Web/API/ResizeObserver
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const observer = new ResizeObserver(() => {
      setOverflowing(list.scrollHeight > list.clientHeight);
    });
    observer.observe(list);
    return () => observer.disconnect();
  }, []);

  return (
    // `min-h-0` is what lets the list become shorter than its items, and
    // so scroll, once the section around it has reached its height limit.
    <ul ref={listRef} className={cx("min-h-0 overflow-y-auto", overflowing && "pr-3")}>
      {children}
    </ul>
  );
}
