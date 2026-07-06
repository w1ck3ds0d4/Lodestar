// Remove native `title` tooltips across every Lodestar window.
//
// WHY: content protection excludes the window's own HWND from screen capture
// (WDA_EXCLUDEFROMCAPTURE), but a `title` tooltip is drawn by Chromium in a SEPARATE top-level
// popup window that does NOT inherit that exclusion. So a tooltip shown on hover could leak into a
// screen share even though the app itself is invisible to capture - exactly the stealth we do not
// want. (The same class of separate-window popups - context menus, <select> dropdowns - would leak
// too; this handles the `title` tooltips, which are the ones the UI actually uses.)
//
// WHAT: strip every element's `title`, moving it onto `aria-label` (screen-reader only, never a
// popup) so accessibility is preserved with nothing to capture. We sweep the current DOM and, via a
// MutationObserver, everything React renders or updates afterwards. Runs once per window at startup;
// because main.tsx is the shared entry for the main window AND the capture-overlay windows, calling
// it there covers them all.

// Elements whose `aria-label` we set ourselves, so we may keep it in sync with a changing `title`
// (e.g. "Sync now" -> "Syncing...") without ever clobbering an aria-label the app set on purpose.
const owned = new WeakSet<Element>();

function stripEl(el: Element): void {
  const t = el.getAttribute("title");
  if (t === null) return;
  if (t && (!el.hasAttribute("aria-label") || owned.has(el))) {
    el.setAttribute("aria-label", t);
    owned.add(el);
  }
  el.removeAttribute("title");
}

function stripTree(root: Element): void {
  stripEl(root);
  root.querySelectorAll("[title]").forEach(stripEl);
}

export function stripNativeTooltips(): void {
  const obs = new MutationObserver((muts) => {
    for (const m of muts) {
      if (m.type === "attributes" && m.target instanceof Element) {
        stripEl(m.target);
      }
      m.addedNodes.forEach((n) => {
        if (n instanceof Element) stripTree(n);
      });
    }
  });
  // documentElement always exists (unlike body when the module script runs early), and observing it
  // with subtree covers the whole document, so nothing React adds later slips through.
  obs.observe(document.documentElement, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["title"],
  });
  stripTree(document.documentElement);
}
