// Capture-safe drop-in replacement for a native <select>.
//
// WHY: a native <select> renders its option list in a SEPARATE top-level popup window that does NOT
// inherit the main window's screen-capture exclusion (WDA_EXCLUDEFROMCAPTURE), so opening one during
// a screen share would leak the options. This renders the list IN the document (a portal into
// document.body of the same webview), so it stays inside the protected window and is never captured.
//
// API mirrors a native select as closely as possible so call sites barely change: pass the same
// `value` and `<option>` children; `onChange` receives the chosen value as a string (like
// `e.target.value`). For numeric selects, pass `value={String(n)}` (or a number - it is coerced) and
// do `Number(v)` in onChange, exactly as the native code already did.

import {
  Children,
  isValidElement,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { ChevronDown } from "lucide-react";

type Option = { value: string; label: ReactNode; disabled: boolean };

type OptionProps = { value?: unknown; children?: ReactNode; disabled?: boolean };

/** Flatten `<option>` children (including those produced by `.map()`) into a plain option list. */
function readOptions(children: ReactNode): Option[] {
  return Children.toArray(children)
    .filter(isValidElement)
    .map((c) => c.props as OptionProps)
    .filter((p) => p.value !== undefined)
    .map((p) => ({
      value: String(p.value),
      label: p.children ?? String(p.value),
      disabled: !!p.disabled,
    }));
}

export function Select({
  value,
  onChange,
  children,
  className,
  ariaLabel,
  disabled,
}: {
  value: string | number;
  onChange: (value: string) => void;
  children: ReactNode;
  className?: string;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  const options = useMemo(() => readOptions(children), [children]);
  const current = String(value);
  const selected = options.find((o) => o.value === current);

  const [open, setOpen] = useState(false);
  const [activeIdx, setActiveIdx] = useState(-1);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [box, setBox] = useState<{ left: number; top: number; width: number; below: boolean } | null>(
    null,
  );
  const listId = useId();

  const place = useCallback(() => {
    const el = triggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    // Open downward unless there is clearly more room above.
    const below = window.innerHeight - r.bottom > 220 || r.top < 220;
    setBox({ left: r.left, top: below ? r.bottom + 4 : r.top - 4, width: r.width, below });
  }, []);

  const openList = useCallback(() => {
    if (disabled) return;
    place();
    const i = options.findIndex((o) => o.value === current);
    setActiveIdx(i >= 0 ? i : 0);
    setOpen(true);
  }, [disabled, place, options, current]);

  const close = useCallback(() => {
    setOpen(false);
    setActiveIdx(-1);
  }, []);

  const choose = useCallback(
    (v: string) => {
      onChange(v);
      close();
      triggerRef.current?.focus();
    },
    [onChange, close],
  );

  // While open: reposition on scroll/resize, and close on any outside pointer press.
  useEffect(() => {
    if (!open) return;
    const reposition = () => place();
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!triggerRef.current?.contains(t) && !listRef.current?.contains(t)) close();
    };
    window.addEventListener("scroll", reposition, true);
    window.addEventListener("resize", reposition);
    document.addEventListener("pointerdown", onDown, true);
    return () => {
      window.removeEventListener("scroll", reposition, true);
      window.removeEventListener("resize", reposition);
      document.removeEventListener("pointerdown", onDown, true);
    };
  }, [open, place, close]);

  const step = (dir: 1 | -1) => {
    if (!options.length) return;
    let i = activeIdx;
    for (let n = 0; n < options.length; n++) {
      i = (i + dir + options.length) % options.length;
      if (!options[i].disabled) break;
    }
    setActiveIdx(i);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openList();
      }
      return;
    }
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        close();
        break;
      case "Enter":
      case " ": {
        e.preventDefault();
        const o = options[activeIdx];
        if (o && !o.disabled) choose(o.value);
        break;
      }
      case "ArrowDown":
        e.preventDefault();
        step(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        step(-1);
        break;
      case "Home":
        e.preventDefault();
        setActiveIdx(options.findIndex((o) => !o.disabled));
        break;
      case "End":
        e.preventDefault();
        setActiveIdx(options.length - 1);
        break;
    }
  };

  const list =
    open && box
      ? createPortal(
          <ul
            ref={listRef}
            id={listId}
            role="listbox"
            className="jr-select__list"
            style={{
              position: "fixed",
              left: box.left,
              width: box.width,
              ...(box.below
                ? { top: box.top }
                : { bottom: window.innerHeight - box.top }),
            }}
          >
            {options.map((o, i) => (
              <li
                key={o.value}
                role="option"
                aria-selected={o.value === current}
                aria-disabled={o.disabled || undefined}
                className={
                  "jr-select__opt" +
                  (i === activeIdx ? " is-active" : "") +
                  (o.value === current ? " is-selected" : "") +
                  (o.disabled ? " is-disabled" : "")
                }
                onMouseEnter={() => !o.disabled && setActiveIdx(i)}
                onPointerDown={(e) => {
                  e.preventDefault(); // keep focus on the trigger; avoid the outside-close handler
                  if (!o.disabled) choose(o.value);
                }}
              >
                {o.label}
              </li>
            ))}
          </ul>,
          document.body,
        )
      : null;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className={"jr-select" + (className ? " " + className : "")}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        disabled={disabled}
        onClick={() => (open ? close() : openList())}
        onKeyDown={onKeyDown}
      >
        <span className="jr-select__value">{selected ? selected.label : ""}</span>
        <ChevronDown size={14} className="jr-select__chev" aria-hidden />
      </button>
      {list}
    </>
  );
}
