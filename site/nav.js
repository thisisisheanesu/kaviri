/*
 * The phone menu.
 *
 * The nav used to be hidden at phone width by a CSS rule that dropped every link but the last
 * one, on the grounds that the section links were reachable by scrolling. That is true of the
 * anchors and false of /docs/ and /play/, which are separate pages and were simply unreachable
 * on a phone.
 *
 * Two things make this worth a file rather than a checkbox hack:
 *
 * 1. `.js` is added to <html> by this script, and the CSS only collapses the nav when it is
 *    there. With JavaScript off or this file blocked, the nav renders as a wrapped row of
 *    links, which is worse looking and still usable. A checkbox hack cannot fail that way
 *    round: it would leave a control that does nothing.
 * 2. A real <button> with aria-expanded is what a screen reader announces as a disclosure. A
 *    label pointing at a hidden checkbox is announced as a checkbox, which it is not.
 */

(function () {
  var head = document.querySelector(".site-head");
  var button = document.getElementById("nav-toggle");
  var nav = document.getElementById("site-nav");
  if (!head || !button || !nav) return;

  document.documentElement.classList.add("js");

  function set(open) {
    head.setAttribute("data-nav", open ? "open" : "closed");
    button.setAttribute("aria-expanded", open ? "true" : "false");
  }

  /*
   * The shut state is in the markup, not set from here. Setting it on load meant the menu
   * was open for the one frame between the stylesheet applying and this file running, and
   * now that the collapse is animated that frame became a 320ms animation of the menu
   * closing itself in front of the reader.
   */
  button.setAttribute("aria-expanded", head.getAttribute("data-nav") === "open" ? "true" : "false");

  button.addEventListener("click", function () {
    set(button.getAttribute("aria-expanded") !== "true");
  });

  // Following a link inside the menu leaves it open behind the new page on a same-page anchor,
  // where there is no navigation to close it.
  nav.addEventListener("click", function (e) {
    if (e.target.closest("a")) set(false);
  });

  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && button.getAttribute("aria-expanded") === "true") {
      set(false);
      button.focus();
    }
  });

  /*
   * Crossing back to desktop width with the menu open leaves aria-expanded="true" on a button
   * that is no longer visible, so the next phone-width open is announced as a close.
   */
  var wide = window.matchMedia("(min-width: 600px)");
  var onWide = function (e) { if (e.matches) set(false); };
  if (wide.addEventListener) wide.addEventListener("change", onWide);
  else if (wide.addListener) wide.addListener(onWide);
})();
