/*
 * The admin surface.
 *
 * One password, held as a Worker secret, exchanged for a signed cookie. Not a user table:
 * there is one administrator and there is going to be one administrator, and every account
 * system that exists to serve one person is a liability with a login page.
 *
 * If ADMIN_PASSWORD is not set the whole surface answers 503 and says how to set it, rather
 * than defaulting to something and being open.
 */

import { pending } from "./stripe.js";
import { retryUnconfirmed } from "./waitlist.js";

const COOKIE = "kaviri_admin";
const TTL = 60 * 60 * 12;

const enc = new TextEncoder();

/** Compare without leaking which character differed through how long the compare took. */
function sameSecret(a, b) {
  const x = enc.encode(String(a));
  const y = enc.encode(String(b));
  // Length is compared separately and non-secretly: the lengths are not the secret, and
  // hashing to a fixed width first would be the alternative for no real gain here.
  if (x.length !== y.length) return false;
  let diff = 0;
  for (let i = 0; i < x.length; i++) diff |= x[i] ^ y[i];
  return diff === 0;
}

async function hmac(env, data) {
  const key = await crypto.subtle.importKey(
    "raw",
    enc.encode(env.ADMIN_PASSWORD),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"]
  );
  const sig = await crypto.subtle.sign("HMAC", key, enc.encode(data));
  return [...new Uint8Array(sig)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function mintCookie(env) {
  const exp = Math.floor(Date.now() / 1000) + TTL;
  return `${exp}.${await hmac(env, String(exp))}`;
}

async function cookieValid(env, raw) {
  if (!raw) return false;
  const [exp, sig] = String(raw).split(".");
  if (!exp || !sig) return false;
  if (Number(exp) < Math.floor(Date.now() / 1000)) return false;
  return sameSecret(sig, await hmac(env, exp));
}

function readCookie(request, name) {
  const all = request.headers.get("Cookie") || "";
  for (const part of all.split(";")) {
    const [k, ...v] = part.trim().split("=");
    if (k === name) return v.join("=");
  }
  return null;
}

const html = (body, status = 200, extra = {}) =>
  new Response(body, {
    status,
    headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store", ...extra },
  });

function page(inner) {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>kaviri admin</title><link rel="icon" href="/favicon.svg">
<link rel="stylesheet" href="/brand/tokens.css"><link rel="stylesheet" href="/style.css">
<link rel="stylesheet" href="/admin.css">
</head><body><div class="wrap adm">${inner}</div></body></html>`;
}

function loginPage(msg) {
  return page(`<h1>kaviri admin</h1>
${msg ? `<p class="err">${msg}</p>` : ""}
<form method="POST" action="/admin/login" class="login">
  <label for="p">Password</label>
  <input id="p" name="password" type="password" autocomplete="current-password" required autofocus>
  <button class="btn btn-primary" type="submit">Sign in</button>
</form>`);
}

function esc(s) {
  return String(s ?? "").replace(/[<>&"]/g, (c) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;", '"': "&quot;" })[c]);
}

async function dashboard(env) {
  const db = env.kaviri_waitlist;
  const { results } = await db
    .prepare("select id, email, note, country, created_at, confirmed_at, confirm_via, confirm_err from waitlist order by id desc limit 500")
    .all();
  const rows = results || [];
  const total = rows.length;
  const confirmed = rows.filter((r) => r.confirmed_at).length;
  const failed = rows.filter((r) => !r.confirmed_at && r.confirm_err).length;

  const body = rows
    .map(
      (r) => `<tr>
  <td class="mono">${r.id}</td>
  <td>${esc(r.email)}</td>
  <td class="note">${esc(r.note || "")}</td>
  <td class="mono">${esc(r.country || "")}</td>
  <td class="mono">${esc(r.created_at)}</td>
  <td class="${r.confirmed_at ? "ok" : r.confirm_err ? "bad" : "pending"}">${
    r.confirmed_at ? `sent, ${esc(r.confirm_via)}` : r.confirm_err ? esc(r.confirm_err.slice(0, 90)) : "pending"
  }</td>
</tr>`
    )
    .join("");

  /*
   * Stripe events that have arrived and not yet been taken by the billing service. This is
   * the number that matters while billing is not deployed: it is the backlog that would
   * otherwise have been silently dropped.
   */
  const owed = await pending(env, 200);

  return page(`<div class="adm-head">
  <h1>Waitlist</h1>
  <div class="adm-actions">
    <a class="btn btn-secondary" href="/admin/export.csv">Export CSV</a>
    <form method="POST" action="/admin/retry"><button class="btn btn-secondary" type="submit">Retry unsent</button></form>
    <form method="POST" action="/admin/logout"><button class="btn btn-secondary" type="submit">Sign out</button></form>
  </div>
</div>
<p class="muted">${total} on the list, ${confirmed} confirmed${failed ? `, <b class="bad">${failed} failed to send</b>` : ""}.
Mail goes out through ${env.RESEND_API_KEY ? "Resend" : env.SMTP_HOST ? "SMTP" : "<b class='bad'>nothing: no sender is configured</b>"}.</p>
<p class="muted">Stripe: ${
    owed.length === 0
      ? "no events waiting."
      : `<b>${owed.length}</b> event${owed.length === 1 ? "" : "s"} received and waiting for the billing service to drain them` +
        `${owed.some((e) => e.livemode) ? ', <b class="bad">including live ones</b>' : " (all test mode)"}.`
  }</p>
<table><thead><tr><th>#</th><th>Email</th><th>Note</th><th>CC</th><th>When</th><th>Confirmation</th></tr></thead>
<tbody>${body || '<tr><td colspan="6" class="muted">Nobody yet.</td></tr>'}</tbody></table>`);
}

export async function handle(request, env, url) {
  if (!env.ADMIN_PASSWORD) {
    return html(
      page(`<h1>Admin is not set up</h1><p>Set the password and this page starts working:</p>
<pre><code>cd site &amp;&amp; npx wrangler secret put ADMIN_PASSWORD</code></pre>
<p class="muted">Nothing here is reachable until it is set, which is the point.</p>`),
      503
    );
  }

  const authed = await cookieValid(env, readCookie(request, COOKIE));

  if (url.pathname === "/admin/login" && request.method === "POST") {
    const form = await request.formData();
    if (!sameSecret(form.get("password") || "", env.ADMIN_PASSWORD)) {
      // One second, always, whether or not the password was right. It makes guessing slow and
      // it makes a wrong guess indistinguishable from a right one by timing.
      await new Promise((r) => setTimeout(r, 1000));
      return html(loginPage("Wrong password."), 401);
    }
    return new Response(null, {
      status: 303,
      headers: {
        Location: "/admin",
        "Set-Cookie": `${COOKIE}=${await mintCookie(env)}; Path=/admin; HttpOnly; Secure; SameSite=Strict; Max-Age=${TTL}`,
      },
    });
  }

  if (url.pathname === "/admin/logout" && request.method === "POST") {
    return new Response(null, {
      status: 303,
      headers: { Location: "/admin", "Set-Cookie": `${COOKIE}=; Path=/admin; HttpOnly; Secure; SameSite=Strict; Max-Age=0` },
    });
  }

  if (!authed) return html(loginPage(null), url.pathname === "/admin" ? 200 : 401);

  if (url.pathname === "/admin/export.csv") {
    const { results } = await env.kaviri_waitlist
      .prepare("select id, email, note, country, created_at, confirmed_at from waitlist order by id")
      .all();
    const cell = (v) => `"${String(v ?? "").replace(/"/g, '""')}"`;
    const csv = ["id,email,note,country,created_at,confirmed_at"]
      .concat((results || []).map((r) => [r.id, r.email, r.note, r.country, r.created_at, r.confirmed_at].map(cell).join(",")))
      .join("\n");
    return new Response(csv, {
      headers: {
        "Content-Type": "text/csv; charset=utf-8",
        "Content-Disposition": 'attachment; filename="kaviri-waitlist.csv"',
        "Cache-Control": "no-store",
      },
    });
  }

  if (url.pathname === "/admin/retry" && request.method === "POST") {
    await retryUnconfirmed(env, 50);
    return new Response(null, { status: 303, headers: { Location: "/admin" } });
  }

  return html(await dashboard(env));
}
