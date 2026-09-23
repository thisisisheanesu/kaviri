/*
 * The waitlist: one table, one public endpoint, and an admin behind a password.
 *
 * It used to be a mailto link, which is a waitlist in the sense that a shoebox is a filing
 * system. This stores the signup, tells the person it worked, and tells the owner it happened.
 */

import { sendMail, senderName, notifyOwner } from "./mail.js";

export const SCHEMA = `
create table if not exists waitlist (
  id           integer primary key autoincrement,
  email        text not null,
  -- Lowercased and trimmed. The unique index is on this rather than on email, so that
  -- Ishe@Example.com and ishe@example.com are one person and the original spelling is
  -- still what gets written to.
  email_key    text not null,
  note         text,
  source       text,
  country      text,
  created_at   text not null default (datetime('now')),
  confirmed_at text,
  confirm_via  text,
  confirm_err  text
);
create unique index if not exists waitlist_email_key on waitlist(email_key);
create index if not exists waitlist_created on waitlist(created_at desc);
`;

/*
 * Deliberately loose. The job of this check is to catch a typo and a bot posting junk, not to
 * decide what an address may look like: every strict email regex ever written rejects somebody
 * real, and the confirmation mail is the thing that actually proves the address works.
 */
const EMAIL = /^[^@\s]+@[^@\s.]+\.[^@\s]{2,}$/;

const json = (body, status = 200, extra = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store", ...extra },
  });

function welcome(email) {
  return `You are on the kaviri waitlist.

kaviri records a demo video from a script, so the video is a build artifact and CI can
re-render it every time the product changes. The recorder is free and open source. The hosted
service takes the script and hands back an MP4 with a stable URL, and that is the part you have
just put your name down for.

You will hear from me when it is ready, and not in between. If you have a demo that is already
out of date and you want to tell me about it, reply to this: it is the most useful thing you
could send.

https://kaviri.dev
https://kaviri.dev/play   (the script, running in your browser)

-- Ishe
`;
}

export async function signup(request, env, ctx) {
  let body;
  try {
    body = await request.json();
  } catch {
    return json({ error: "expected a JSON body" }, 400);
  }

  const email = String(body.email || "").trim();
  const note = String(body.note || "").trim().slice(0, 500);
  if (!EMAIL.test(email) || email.length > 254) {
    return json({ error: "that does not look like an email address" }, 400);
  }
  /*
   * A hidden field no human fills in. Answering 200 rather than 400 matters: a bot that is
   * told it failed tries something else, and one that is told it worked goes away.
   */
  if (body.website) return json({ ok: true, already: false });

  const key = email.toLowerCase();
  const db = env.kaviri_waitlist;

  const existing = await db.prepare("select id from waitlist where email_key = ?").bind(key).first();
  if (existing) return json({ ok: true, already: true });

  await db
    .prepare("insert into waitlist (email, email_key, note, source, country) values (?, ?, ?, ?, ?)")
    .bind(email, key, note || null, String(body.source || "site").slice(0, 40), request.cf?.country || null)
    .run();

  /*
   * The mail goes out after the response. A signup that is recorded but whose confirmation is
   * slow is a small problem; a form that spins for four seconds because an SMTP server is
   * thinking is the one the person actually experiences.
   */
  /*
   * Whatever happens in here, the row gets a verdict. A rejected waitUntil is invisible: no
   * log anybody reads, and a row that stays null forever looks like one the cron has not got
   * to yet.
   */
  ctx.waitUntil(
    confirm(env, email, key, note).catch((e) =>
      db
        .prepare("update waitlist set confirm_err = ? where email_key = ?")
        .bind(`confirm threw: ${String(e && e.message ? e.message : e)}`.slice(0, 300), key)
        .run()
        .catch(() => {})
    )
  );
  return json({ ok: true, already: false, mail: senderName(env) ? "queued" : "not configured" });
}

async function confirm(env, email, key, note) {
  const db = env.kaviri_waitlist;
  const r = await sendMail(env, {
    to: email,
    subject: "You are on the kaviri waitlist",
    text: welcome(email),
    replyTo: "hello@kaviri.dev",
  });
  /*
   * Plain positional placeholders. D1 binds by position, and an earlier version of this used
   * SQLite's numbered ?1 form, which threw at bind time: the send result was then never
   * written, so a row that had in fact failed to send looked identical to one that had never
   * been attempted. The whole point of these three columns is to tell those two apart.
   */
  await db
    .prepare(
      "update waitlist set confirmed_at = ?, confirm_via = ?, confirm_err = ? where email_key = ?"
    )
    .bind(r.sent ? new Date().toISOString().replace("T", " ").slice(0, 19) : null, r.via, r.error || null, key)
    .run();

  /*
   * And tell the owner. This goes through Cloudflare's binding rather than the configured
   * sender, so it keeps working when there is no sender at all: every signup is known about
   * immediately even while the confirmations are piling up unsent.
   */
  await notifyOwner(env, {
    subject: `kaviri waitlist: ${email}`,
    text: `${email}\n\n${note || "(no note)"}\n\n${r.sent ? `Confirmed via ${r.via}.` : `CONFIRMATION NOT SENT: ${r.error}`}\n\nhttps://kaviri.dev/admin`,
  }).catch(() => {});
}

/** Anything the confirmation could not be sent for, retried on a schedule. */
export async function retryUnconfirmed(env, limit = 20) {
  if (!senderName(env)) return { retried: 0, reason: "no sender configured" };
  const { results } = await env.kaviri_waitlist
    .prepare("select email, email_key, note from waitlist where confirmed_at is null order by id limit ?")
    .bind(limit)
    .all();
  let ok = 0;
  for (const row of results || []) {
    const before = await env.kaviri_waitlist
      .prepare("select confirmed_at from waitlist where email_key = ?")
      .bind(row.email_key)
      .first();
    if (before?.confirmed_at) continue;
    await confirm(env, row.email, row.email_key, row.note);
    ok++;
  }
  return { retried: ok };
}

export { json };
