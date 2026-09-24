/*
 * The waitlist sequence.
 *
 * Five emails over about three and a half weeks, then silence until the hosted service is
 * real. They are here rather than in a mail provider's visual editor for the same reason the
 * demo script is in the repo: a sequence you can only see by logging into a dashboard is one
 * nobody reviews, and one nobody can diff.
 *
 * The shape of the thing:
 *
 *   step 0   day 0    the confirmation, sent inline by the signup handler
 *   step 1   day 2    record one, locally, in two minutes
 *   step 2   day 6    put it in CI, which is the actual claim
 *   step 3   day 13   the agent story, which is the part nobody else has
 *   step 4   day 24   what I am building and what I want to know
 *
 * Rules this obeys, because a drip sequence that does not is spam:
 *
 * - Every step earns its place by being useful about the free open-source recorder. Only the
 *   last one asks for anything, and what it asks for is a reply.
 * - Nobody is mailed a step until their confirmation has actually been delivered. A person
 *   whose welcome bounced should not then receive four more.
 * - One step per person per run. A signup from three weeks ago does not get the whole
 *   sequence in one go when this ships; they get it an hour apart, which is still wrong but is
 *   recoverable, and the admin shows it happening.
 * - Unsubscribe is one click, is honoured before anything else, and is in every step.
 */

import { render, p, lead, h, code, button, link, rule, note } from "./brand-email.js";
import { sendMail, senderName } from "./mail.js";
import { SCHEMA as META_SCHEMA } from "./stats.js";
import { SCHEMA as VISIT_SCHEMA } from "./visits.js";

const SITE = "https://kaviri.dev";
const REPO = "https://github.com/thisisisheanesu/kaviri";

export const SCHEMA = `
create table if not exists waitlist_sends (
  email_key  text not null,
  step       integer not null,
  sent_at    text,
  via        text,
  error      text,
  attempts   integer not null default 0,
  updated_at text not null default (datetime('now')),
  primary key (email_key, step)
);
create index if not exists waitlist_sends_step on waitlist_sends(step, sent_at);
`;

/*
 * Columns added to an existing waitlist table. D1 has no "add column if not exists", and
 * re-running an ALTER that has already been applied is an error rather than a no-op, so the
 * caller swallows it. See ensureColumns.
 */
const COLUMNS = [
  "alter table waitlist add column unsub_token text",
  "alter table waitlist add column unsubscribed_at text",
];

/**
 * Bring an existing database up to what this file needs.
 *
 * Called from the cron rather than from the request path: it is two statements that almost
 * always do nothing, and doing nothing on every page view is still doing it.
 */
export async function ensureSchema(env) {
  const db = env.kaviri_waitlist;
  for (const stmt of (SCHEMA + META_SCHEMA + VISIT_SCHEMA).split(";").map((s) => s.trim()).filter(Boolean)) {
    await db.prepare(stmt).run();
  }
  for (const stmt of COLUMNS) {
    // "duplicate column name" is the expected outcome on every run after the first.
    await db.prepare(stmt).run().catch(() => {});
  }
  // Backfill a token for anyone who signed up before unsubscribe existed.
  const { results } = await db
    .prepare("select email_key from waitlist where unsub_token is null limit 200")
    .all()
    .catch(() => ({ results: [] }));
  for (const row of results || []) {
    await db
      .prepare("update waitlist set unsub_token = ? where email_key = ? and unsub_token is null")
      .bind(newToken(), row.email_key)
      .run()
      .catch(() => {});
  }
}

/*
 * ensureSchema, but at most once per isolate.
 *
 * The cron is what normally applies this, and the cron runs at :17. Between a deploy and that
 * minute there is a window where the code expects `unsub_token` and the table does not have
 * it, and in that window a signup would fail on the insert and /admin would 500 on the select.
 * A one-shot guard on the request path closes it for the cost of two no-op statements the
 * first time an isolate handles one of those two routes.
 *
 * The promise is cached rather than a boolean, so two concurrent requests in a cold isolate
 * wait on the same work instead of both doing it. A failure is not cached: it clears the
 * promise so the next request tries again rather than being stuck for the isolate's life.
 */
let ensured = null;

export function ensureSchemaOnce(env) {
  if (!ensured) {
    ensured = ensureSchema(env).catch((e) => {
      ensured = null;
      throw e;
    });
  }
  return ensured;
}

/**
 * A random per-person unsubscribe token.
 *
 * Stored rather than derived. An HMAC over the address would need a secret to exist and to
 * never change, and it could not be revoked without changing it for everybody. A row holds a
 * random string, the link carries it, and rotating one person's is an update.
 */
export function newToken() {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
}

export function unsubscribeUrl(token) {
  return token ? `${SITE}/unsubscribe?t=${token}` : null;
}

/* ------------------------------------------------------------------- steps */

/**
 * Each step is `{ after, subject, preheader, blocks }`.
 *
 * `after` is days from the signup, not days from the previous step, so inserting a step or
 * changing a delay cannot silently shift everything after it.
 */
export const STEPS = [
  /* 0 is the confirmation. It is sent inline at signup, not by the scheduler, so that the
     person sees it while they still remember filling the form in. It lives here so that all
     five are written in one place and the scheduler can still count it. */
  {
    after: 0,
    inline: true,
    subject: "You are on the kaviri waitlist",
    preheader: "The recorder is free and open source. You can use it today.",
    blocks: [
      lead("You are on the list for the hosted service. That is the whole of what you signed up for, and it is not open yet."),
      p("In the meantime the thing that makes the video is free, open source under Apache 2.0, and works today on your laptop with no account:"),
      code("cargo install kaviri\nkaviri record --script take.jsonl --out take.mp4"),
      p("kaviri drives its own Chrome over CDP, so there is no screen recorder, no window to keep in focus and no permission dialog. It runs the same on a laptop and on a CI runner with no screen at all."),
      button("Try it in the browser first", `${SITE}/play/`),
      rule(),
      p("Over the next few weeks I will send four more: how to record your first take, how to make CI fail when the demo goes stale, how to drive it from an agent, and one at the end asking what you actually need. Then nothing until the hosted service is real."),
      p("If you want none of that, the unsubscribe link below takes you off everything, including the launch email. One click, no page to fill in."),
      p("*Ishe*"),
    ],
  },

  {
    after: 2,
    subject: "Record your first take in about two minutes",
    preheader: "Eleven lines of JSON and one command.",
    blocks: [
      lead("A take is a newline-delimited JSON file. One object per line, one op each."),
      p("Here is a complete one. It opens a page, waits for the app to actually be up, types into a field, clicks, and waits long enough for the last interaction to be worth filming:"),
      code(`{"op":"navigate","url":"http://127.0.0.1:8099/"}
{"op":"wait","selector":"#q","timeout_ms":30000}
{"op":"start_recording"}
{"op":"wait","ms":500}
{"op":"type","selector":"#q","text":"14 Rue Lafayette"}
{"op":"click","selector":"#go"}
{"op":"wait","selector":"li","timeout_ms":15000}
{"op":"wait","ms":1700}
{"op":"stop_recording"}`),
      code("kaviri record --script take.jsonl --out take.mp4"),
      p("You will notice there is no camera in that file. There is no `zoom`, no `pan`, no keyframe. The camera is derived: every `click` and `type` carries a bounding box, and a critically damped spring moves the frame onto it and settles without overshooting."),
      note("The one mistake worth avoiding on the first try: `start_recording` goes *after* the `wait` that proves the app is up. Put it before and the first two seconds are a blank page."),
      link("The full op reference", `${REPO}/blob/main/docs/script-protocol.md`),
      p("*Ishe*"),
    ],
  },

  {
    after: 6,
    subject: "The part that actually matters: making the build fail",
    preheader: "A recorder that films a Chrome error page has not failed in any way CI can see.",
    blocks: [
      lead("Recording a video from a script is a convenience. Recording it in CI is the point."),
      p("A demo goes stale because re-recording it means blocking out an afternoon. If the take is a file in the repo, the video is regenerated on every push that touches the UI, and it is never out of date because nobody has to remember it."),
      h("The trap"),
      p("A recorder that writes thirty valid seconds of a frozen dev-server timeout has not failed in any way your CI can see. It exits zero, the MP4 is well formed, and a duration check sails straight past it."),
      p("So kaviri's own workflow compares the first frame against the last one. A script that navigates, clicks and types cannot produce the same picture twice:"),
      code(`psnr=$(... first.png vs last.png ...)
if [ "$psnr" = "inf" ]; then
  echo "the take filmed nothing"
  exit 1
fi`),
      p("That line in a pull request is the whole idea. The demo stops being a file someone remembers to refresh and becomes a check that goes red."),
      button("The CI recipe", `${SITE}/#ci`),
      p("*Ishe*"),
    ],
  },

  {
    after: 13,
    subject: "If you are pointing an agent at this",
    preheader: "Serve mode: NDJSON in, one result line per op out, the browser stays open.",
    blocks: [
      lead("An increasing number of the people reading this are not people."),
      p("kaviri was built so that a coding agent can produce a demo video without a human watching. Every op answers on stdout with `{\"ok\":true,...}` or `{\"ok\":false,\"error\":\"...\"}`, and the process exits non-zero if any op failed, so success and failure are readable without watching the video."),
      h("Serve mode"),
      code("kaviri serve"),
      p("NDJSON ops on stdin, one JSON result line per op on stdout. The browser stays open between ops, so an agent can interleave its own reasoning, read each result, and decide the next op from what actually happened rather than from what it hoped would happen."),
      p("The single most useful thing to assert on is `zoom_events`. If it comes back `0`, no `click` or `type` ever resolved a bounding box, the take is a flat screen recording with no camera work, and the script is broken even though nothing errored."),
      note("If a modal or a loading overlay is over the target, kaviri refuses rather than clicking through it, and says which element is in the way. That message means the page is not in the state the script assumes."),
      link("The whole agent-facing document, which is short on purpose", `${SITE}/docs/agents/`),
      p("*Ishe*"),
    ],
  },

  {
    after: 24,
    subject: "What would you actually pay for?",
    preheader: "One question, and a reply goes straight to me.",
    blocks: [
      lead("Last one until the hosted service exists. This is the one where I ask you something."),
      p("The recorder is finished enough to use and it is free forever. What I do not know is which part of the hosted version is worth money to you, because the honest answer is that I have guesses:"),
      p("A queue, so a take does not occupy a CI minute. Storage and a stable URL you can point a README at. Every take kept, so you can see what the demo looked like three releases ago. Rendering on a machine that is not your CI runner, which for a long take is the difference between two minutes and twenty."),
      h("The question"),
      p("Which of those, if any, would make you move a real demo onto it? And what is the demo? A reply to this email comes straight to me, and a one-line answer is more useful than no answer."),
      p("If none of it is worth paying for, that is also worth knowing and I would rather hear it now."),
      rule(),
      p("Either way you will hear once more, when the hosted service opens. Nothing between now and then."),
      p("*Ishe*"),
    ],
  },
];

/** The reason line in the footer, which is how a person works out why they got this. */
const REASON = "You are getting this because you joined the kaviri waitlist at kaviri.dev.";

/** Build one step's `{ subject, html, text }` for one person. */
export function compose(step, token) {
  const s = STEPS[step];
  if (!s) throw new Error(`no step ${step}`);
  const { html, text } = render({
    preheader: s.preheader,
    blocks: s.blocks,
    unsubscribeUrl: unsubscribeUrl(token),
    reason: REASON,
  });
  return { subject: s.subject, html, text };
}

/* --------------------------------------------------------------- the runner */

/**
 * How many people to mail in one cron run.
 *
 * Resend's free tier is 100 a day and about two requests a second. Twenty an hour cannot
 * breach either, and a list that grows past what twenty an hour can carry is a list worth
 * paying for a plan over.
 */
const PER_RUN = 20;

/**
 * How many times one step is retried for one person before it is left alone.
 *
 * Five hourly attempts is a provider outage survived. Past that it is an address that will
 * never accept mail, and retrying it forever costs quota that a real signup needs.
 */
const MAX_ATTEMPTS = 5;

const DAY_MS = 86400000;

/**
 * Send whatever is due.
 *
 * Runs on the hourly cron that already retries failed confirmations. Returns a small summary
 * rather than nothing, because the admin page shows it and "it ran and did nothing" and "it
 * did not run" are different problems.
 */
export async function runSequence(env, limit = PER_RUN) {
  if (!senderName(env)) return { sent: 0, reason: "no sender configured" };
  const db = env.kaviri_waitlist;

  /*
   * Only people whose confirmation actually went out, and who have not unsubscribed. The left
   * join gives, per person, the highest step already recorded as sent; the next one is the
   * candidate. Doing this in SQL rather than in a loop keeps the number of D1 round trips
   * proportional to the number of mails rather than to the size of the list.
   */
  const { results } = await db
    .prepare(
      `select w.email, w.email_key, w.unsub_token, w.created_at,
              coalesce(max(s.step), -1) as last_step
         from waitlist w
         left join waitlist_sends s
                on s.email_key = w.email_key and s.sent_at is not null
        where w.confirmed_at is not null
          and w.unsubscribed_at is null
        group by w.email_key
        order by w.id`
    )
    .all()
    .catch(() => ({ results: [] }));

  const now = Date.now();
  let sent = 0;
  let due = 0;

  for (const row of results || []) {
    if (sent >= limit) break;

    const next = Number(row.last_step) + 1;
    if (next >= STEPS.length) continue;
    // Step 0 is sent inline at signup. A row whose confirmation succeeded has had it, even
    // though nothing wrote a waitlist_sends row for it at the time.
    const step = next === 0 ? 1 : next;
    if (step >= STEPS.length) continue;

    /*
     * created_at is SQLite's `YYYY-MM-DD HH:MM:SS` in UTC with no zone marker, which
     * Date.parse reads as local time. The Z is what stops a signup looking hours older or
     * younger than it is depending on where the Worker ran.
     */
    const startedAt = Date.parse(String(row.created_at).replace(" ", "T") + "Z");
    if (!Number.isFinite(startedAt)) continue;
    if (now - startedAt < STEPS[step].after * DAY_MS) continue;

    due++;

    /*
     * Claim the step before sending, and treat the claim as failed unless a row actually
     * changed. Two overlapping runs would otherwise both find it due and the person would get
     * the same email twice; the `where sent_at is null` guard makes the second one a no-op,
     * and `meta.changes` is the only way to tell a no-op from a claim, because the statement
     * succeeds either way.
     *
     * MAX_ATTEMPTS stops an address that will never accept mail from being retried hourly
     * until the heat death of the universe. Five failures and the step is left alone; the
     * admin shows it with its last error.
     */
    const claim = await db
      .prepare(
        `insert into waitlist_sends (email_key, step, attempts, updated_at)
              values (?, ?, 1, datetime('now'))
         on conflict(email_key, step) do update
              set attempts = attempts + 1, updated_at = datetime('now')
            where waitlist_sends.sent_at is null
              and waitlist_sends.attempts < ${MAX_ATTEMPTS}`
      )
      .bind(row.email_key, step)
      .run()
      .catch(() => null);
    if (!claim || !claim.meta || claim.meta.changes !== 1) continue;

    const mail = compose(step, row.unsub_token);
    const r = await sendMail(env, {
      to: row.email,
      subject: mail.subject,
      text: mail.text,
      html: mail.html,
      replyTo: "hello@kaviri.dev",
      listUnsubscribe: unsubscribeUrl(row.unsub_token),
    });

    await db
      .prepare(
        "update waitlist_sends set sent_at = ?, via = ?, error = ?, updated_at = datetime('now') where email_key = ? and step = ?"
      )
      .bind(
        r.sent ? new Date().toISOString().replace("T", " ").slice(0, 19) : null,
        r.via,
        r.error ? String(r.error).slice(0, 300) : null,
        row.email_key,
        step
      )
      .run()
      .catch(() => {});

    if (r.sent) sent++;
  }

  return { sent, due, considered: (results || []).length };
}

/* ------------------------------------------------------------- unsubscribe */

const page = (title, body) =>
  new Response(
    `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title}</title><link rel="icon" href="/favicon.svg">
<link rel="stylesheet" href="/brand/tokens.css"><link rel="stylesheet" href="/style.css">
</head><body><main class="wrap"><section class="wrap">${body}</section></main></body></html>`,
    { status: 200, headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" } }
  );

/**
 * One click, no confirmation step, no login.
 *
 * A POST is handled too and answered with 200 and no body: that is what
 * `List-Unsubscribe-Post: List-Unsubscribe=One-Click` means, and it is what makes Gmail show
 * its own unsubscribe control next to the sender rather than sending the reader hunting for a
 * link at the bottom. An unsubscribe that takes two clicks gets a spam report instead.
 */
export async function unsubscribe(request, env, url) {
  const token = url.searchParams.get("t") || "";
  const oneClick = request.method === "POST";

  if (!/^[0-9a-f]{32}$/.test(token)) {
    return oneClick
      ? new Response(null, { status: 400 })
      : page("kaviri", `<h2>That link is not one of mine.</h2>
<p class="muted">Mail <a href="mailto:hello@kaviri.dev">hello@kaviri.dev</a> and I will take you off by hand.</p>`);
  }

  const r = await env.kaviri_waitlist
    .prepare("update waitlist set unsubscribed_at = datetime('now') where unsub_token = ? and unsubscribed_at is null")
    .bind(token)
    .run()
    .catch(() => null);

  if (oneClick) return new Response(null, { status: r ? 200 : 500 });

  return page(
    "Unsubscribed",
    `<h2>Done. You will not hear from me again.</h2>
<p class="muted">That is everything, including the email when the hosted service opens. Your address stays in the list marked as unsubscribed rather than being deleted, which is what stops a later import quietly putting you back on.</p>
<p class="muted">If that was a misclick, mail <a href="mailto:hello@kaviri.dev">hello@kaviri.dev</a> and it is a one-line job to undo.</p>
<p><a class="btn btn-secondary" href="/">Back to kaviri.dev</a></p>`
  );
}
