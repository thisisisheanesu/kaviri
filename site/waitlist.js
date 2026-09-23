/*
 * The waitlist form.
 *
 * Progressive in the only way that matters here: the form posts JSON and replaces itself with
 * a line of text, and if this file fails to load the form still has a required email field and
 * a submit button that does nothing, which is a broken form rather than a form that looks like
 * it worked. There is no server-rendered fallback because there is no server-rendered page.
 */

(function () {
  const form = document.getElementById("wl");
  if (!form) return;
  const msg = document.getElementById("wl-msg");
  const button = form.querySelector("button[type=submit]");

  function say(text, bad) {
    msg.textContent = text;
    msg.className = "wl-msg" + (bad ? " bad" : " ok");
  }

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const email = form.email.value.trim();
    // The same loose check the Worker does, so the common typo is caught without a round trip.
    if (!/^[^@\s]+@[^@\s.]+\.[^@\s]{2,}$/.test(email)) {
      say("That does not look like an email address.", true);
      form.email.focus();
      return;
    }

    button.disabled = true;
    const was = button.textContent;
    button.textContent = "...";
    try {
      const r = await fetch("/api/waitlist", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          email,
          note: form.note.value.trim(),
          website: form.website.value,
          source: "site",
        }),
      });
      const j = await r.json().catch(() => ({}));
      if (!r.ok) {
        say(j.error || "That did not work. Try again, or mail hello@kaviri.dev.", true);
        return;
      }
      if (j.already) {
        say("You are already on the list. Nothing more to do.", false);
      } else {
        say("You are on the list. Check your email.", false);
      }
      form.email.disabled = true;
      form.note.disabled = true;
      button.remove();
    } catch {
      say("That did not work. Try again, or mail hello@kaviri.dev.", true);
    } finally {
      button.disabled = false;
      button.textContent = was;
    }
  });
})();
