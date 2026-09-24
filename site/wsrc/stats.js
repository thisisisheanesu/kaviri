/*
 * The numbers behind /admin.
 *
 * Separate from admin.js because the two jobs are different: admin.js decides who may look,
 * this decides what there is to look at. It reads and never writes, so it can be pointed at a
 * copy of the database without anyone having to check what it might do.
 *
 * Every figure here answers a question that would otherwise be answered by guessing:
 *
 *   is the list growing, and did the thing I posted yesterday do anything
 *   are the confirmations actually arriving, and if not what is the error
 *   where in the five step sequence do people stop, and do they leave at a particular one
 *   is the cron still running
 *
 * The last one is the one that matters most and is the easiest to forget: everything else on
 * this page can look healthy while nothing has run for a week.
 */

/* ------------------------------------------------------------------ reading */

/** One place for "this query may not have a table yet", which is true on a cold database. */
async function rows(db, sql, ...binds) {
  try {
    const r = await db.prepare(sql).bind(...binds).all();
    return r.results || [];
  } catch {
    return [];
  }
}

async function one(db, sql, ...binds) {
  try {
    return (await db.prepare(sql).bind(...binds).first()) || {};
  } catch {
    return {};
  }
}

export async function collect(env, steps) {
  const db = env.kaviri_waitlist;

  const [
    totals,
    perDay,
    countries,
    sources,
    byProvider,
    failures,
    sequence,
    churn,
    speed,
    stripeTypes,
    stripeOldest,
    heartbeats,
    traffic,
    perDayViews,
    paths,
    referrers,
    visitorCountries,
    devices,
    entries,
  ] = await Promise.all([
    one(
      db,
      `select count(*) total,
              sum(case when confirmed_at is not null then 1 else 0 end) confirmed,
              sum(case when confirmed_at is null and confirm_err is not null then 1 else 0 end) failed,
              sum(case when unsubscribed_at is not null then 1 else 0 end) gone,
              min(created_at) first_at,
              max(created_at) last_at
         from waitlist`
    ),
    rows(
      db,
      `select date(created_at) d, count(*) n from waitlist
        where created_at >= date('now','-29 days') group by d order by d`
    ),
    rows(
      db,
      `select coalesce(nullif(country,''),'unknown') k, count(*) n from waitlist
        group by k order by n desc, k limit 12`
    ),
    rows(
      db,
      `select coalesce(nullif(source,''),'unknown') k, count(*) n from waitlist
        group by k order by n desc, k limit 12`
    ),
    rows(
      db,
      `select coalesce(confirm_via,'not sent') k, count(*) n from waitlist group by k order by n desc`
    ),
    rows(
      db,
      `select confirm_err k, count(*) n from waitlist
        where confirmed_at is null and confirm_err is not null group by k order by n desc limit 8`
    ),
    rows(
      db,
      `select step,
              count(*) attempted,
              sum(case when sent_at is not null then 1 else 0 end) sent,
              sum(case when sent_at is null then 1 else 0 end) stuck,
              max(attempts) worst,
              max(case when sent_at is null then error end) last_error
         from waitlist_sends group by step order by step`
    ),
    /*
     * Where people leave. The step they had last been sent when they unsubscribed, which is
     * the only way to see that, say, everyone goes after step 3. Step 0 is the welcome, which
     * everybody confirmed gets, so a churn row at 0 means they left before the sequence began.
     */
    rows(
      db,
      `select coalesce((select max(s.step) from waitlist_sends s
                         where s.email_key = w.email_key and s.sent_at is not null), 0) k,
              count(*) n
         from waitlist w where w.unsubscribed_at is not null group by k order by k`
    ),
    one(
      db,
      `select avg((julianday(confirmed_at) - julianday(created_at)) * 86400.0) avg_s,
              max((julianday(confirmed_at) - julianday(created_at)) * 86400.0) max_s
         from waitlist where confirmed_at is not null`
    ),
    rows(
      db,
      `select type k, count(*) n,
              sum(case when drained_at is null then 1 else 0 end) waiting,
              sum(livemode) live
         from stripe_events group by k order by n desc limit 12`
    ),
    one(db, `select min(received_at) oldest from stripe_events where drained_at is null`),
    rows(db, `select k, v, at from meta where k not like 'salt:%'`),
    one(
      db,
      `select (select coalesce(sum(n),0) from views) views,
              (select count(*) from visits) visitors,
              (select coalesce(sum(n),0) from views where d >= date('now','-29 days')) views30,
              (select count(*) from visits where d >= date('now','-29 days')) visitors30,
              (select count(*) from visits where d = date('now')) today,
              (select min(d) from visits) since`
    ),
    rows(
      db,
      `select v.d d, coalesce(vw.n, 0) views, count(*) visitors
         from visits v
         left join (select d, sum(n) n from views group by d) vw on vw.d = v.d
        where v.d >= date('now','-29 days') group by v.d order by v.d`
    ),
    rows(
      db,
      `select path k, sum(n) n from views where d >= date('now','-29 days')
        group by k order by n desc limit 12`
    ),
    rows(
      db,
      `select coalesce(nullif(referrer,''),'direct') k, count(*) n from visits
        where d >= date('now','-29 days') group by k order by n desc limit 12`
    ),
    rows(
      db,
      `select coalesce(nullif(country,''),'unknown') k, count(*) n from visits
        where d >= date('now','-29 days') group by k order by n desc limit 12`
    ),
    rows(
      db,
      `select coalesce(nullif(device,''),'unknown') k, count(*) n from visits
        where d >= date('now','-29 days') group by k order by n desc`
    ),
    rows(
      db,
      `select coalesce(nullif(entry,''),'/') k, count(*) n from visits
        where d >= date('now','-29 days') group by k order by n desc limit 8`
    ),
  ]);

  const meta = Object.fromEntries((heartbeats || []).map((r) => [r.k, { v: r.v, at: r.at }]));

  return {
    totals,
    perDay: fillDays(perDay, 30),
    countries,
    sources,
    byProvider,
    failures,
    sequence: steps.map((s, i) => ({
      step: i,
      after: s.after,
      subject: s.subject,
      ...(sequence.find((r) => r.step === i) || { attempted: 0, sent: 0, stuck: 0, worst: 0 }),
    })),
    churn,
    speed,
    stripeTypes,
    stripeOldest: stripeOldest.oldest || null,
    meta,
    traffic,
    perDayViews: fillTraffic(perDayViews, 30),
    paths,
    referrers,
    visitorCountries,
    devices,
    entries,
    sender: env.RESEND_API_KEY ? "Resend" : env.SMTP_ENABLED === "1" && env.SMTP_HOST ? "SMTP" : null,
  };
}

/**
 * Days with no signups have no row, and a bar chart that silently omits them lies about the
 * shape: four signups on four scattered days reads as four days of steady growth.
 */
function fillDays(found, n) {
  const have = new Map(found.map((r) => [r.d, r.n]));
  const out = [];
  const now = new Date();
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date(now.getTime() - i * 86400000).toISOString().slice(0, 10);
    out.push({ d, n: have.get(d) || 0 });
  }
  return out;
}

/** Same reason as fillDays: a missing day is a zero, not an absence. */
function fillTraffic(found, n) {
  const have = new Map(found.map((r) => [r.d, r]));
  const out = [];
  const now = new Date();
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date(now.getTime() - i * 86400000).toISOString().slice(0, 10);
    const r = have.get(d);
    out.push({ d, views: r?.views || 0, visitors: r?.visitors || 0 });
  }
  return out;
}

/* ----------------------------------------------------------------- drawing */

/*
 * Charts are HTML and CSS. No library, no canvas, no script: the admin page runs under the
 * site's CSP, which allows no inline script, and a bar is a div with a width. Every chart here
 * is one measure, so there is one colour and length carries the value. Nothing is encoded by
 * colour alone, and every number is printed next to its bar rather than hidden behind a hover,
 * because this page is read on a phone as often as not.
 */

const esc = (s) =>
  String(s ?? "").replace(/[<>&"]/g, (c) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;", '"': "&quot;" })[c]);

const pct = (a, b) => (b ? Math.round((a / b) * 100) : 0);

/** A ranked list of bars. One measure, so one hue; the length is the encoding. */
function barList(items, { empty = "Nothing yet." } = {}) {
  if (!items.length) return `<p class="muted">${esc(empty)}</p>`;
  const max = Math.max(...items.map((i) => i.n), 1);
  return `<div class="bars">${items
    .map(
      (i) => `<div class="bar-row">
  <div class="bar-k" title="${esc(i.k)}">${esc(i.k)}</div>
  <div class="bar-track"><div class="bar-fill" style="width:${Math.max((i.n / max) * 100, 1.5)}%"></div></div>
  <div class="bar-n">${i.n}</div>
</div>`
    )
    .join("")}</div>`;
}

/**
 * Thirty days of signups as columns.
 *
 * A column per day rather than a line, because the measure is a count of discrete events and
 * a line between two days implies a value in between that does not exist. Only the ends and
 * the peak are labelled: a number on all thirty is unreadable and is the thing that makes
 * small charts look busy.
 */
function dayChart(days, noun = "signups", key = "n") {
  const max = Math.max(...days.map((d) => d[key]), 1);
  const peak = days.reduce((a, b) => (b[key] > a[key] ? b : a), days[0] || { [key]: 0 });
  const total = days.reduce((a, b) => a + b[key], 0);
  return `<div class="spark" role="img" aria-label="${total} ${noun} over the last 30 days, busiest day ${esc(peak.d)} with ${peak[key]}">
  ${days
    .map(
      (d) =>
        `<div class="spark-col" title="${esc(d.d)}: ${d[key]} ${esc(noun)}"><div class="spark-fill${
          d[key] === 0 ? " zero" : ""
        }" style="height:${d[key] === 0 ? 2 : Math.max((d[key] / max) * 100, 6)}%"></div></div>`
    )
    .join("")}
</div>
<div class="spark-axis"><span>${esc(days[0]?.d || "")}</span><span class="muted">${total} ${esc(noun)} in 30 days, busiest ${peak[key]}</span><span>${esc(
    days[days.length - 1]?.d || ""
  )}</span></div>`;
}

function tile(label, value, note) {
  return `<div class="tile"><div class="tile-n">${esc(value)}</div><div class="tile-k">${esc(label)}</div>${
    note ? `<div class="tile-note">${note}</div>` : ""
  }</div>`;
}

const ago = (iso) => {
  if (!iso) return "never";
  const t = Date.parse(String(iso).replace(" ", "T") + "Z");
  if (!Number.isFinite(t)) return esc(iso);
  const s = Math.max(0, (Date.now() - t) / 1000);
  if (s < 90) return `${Math.round(s)}s ago`;
  if (s < 5400) return `${Math.round(s / 60)} min ago`;
  if (s < 172800) return `${Math.round(s / 3600)}h ago`;
  return `${Math.round(s / 86400)} days ago`;
};

const secs = (v) => {
  if (v == null) return "n/a";
  if (v < 90) return `${v.toFixed(1)}s`;
  if (v < 5400) return `${(v / 60).toFixed(1)} min`;
  if (v < 172800) return `${(v / 3600).toFixed(1)}h`;
  return `${(v / 86400).toFixed(1)} days`;
};

export function render(s) {
  const t = s.totals || {};
  const total = t.total || 0;
  const tr = s.traffic || {};
  const signups30 = (s.perDay || []).reduce((a, b) => a + b.n, 0);

  /*
   * The cron is the thing whose silence is invisible. It runs at :17, so anything past about
   * 75 minutes means it has missed one, and every other number on this page is then stale
   * rather than calm.
   */
  const cronAt = s.meta.last_cron?.at;
  const cronAge = cronAt ? (Date.now() - Date.parse(cronAt.replace(" ", "T") + "Z")) / 60000 : Infinity;
  /*
   * Late and never-run are different states and only one of them is an alarm. A deploy that
   * has not yet reached :17 has no heartbeat, and shouting about it on a page that is fine
   * teaches you to ignore the tile that will one day be telling the truth.
   */
  const cronSeen = Boolean(cronAt);
  const cronLate = cronSeen && cronAge >= 75;

  return `
<section class="panel">
  <div class="tiles">
    ${tile("visitors", tr.visitors30 || 0, `${tr.views30 || 0} page views, 30 days`)}
    ${tile("today", tr.today || 0, "visitors so far")}
    ${tile(
      "sign up rate",
      tr.visitors30 ? `${pct(signups30, tr.visitors30)}%` : "n/a",
      tr.visitors30
        ? `${signups30} of ${tr.visitors30} visitors in 30 days`
        : "no visitors recorded yet"
    )}
    ${tile("on the list", total)}
    ${tile("confirmed", t.confirmed || 0, `<span class="${t.confirmed === total ? "ok" : ""}">${pct(t.confirmed || 0, total)}% of the list</span>`)}
    ${tile("failed to send", t.failed || 0, t.failed ? '<span class="bad">needs a retry</span>' : "none")}
    ${tile("unsubscribed", t.gone || 0, `${pct(t.gone || 0, total)}% of the list`)}
    ${tile("sequence mails out", (s.sequence || []).reduce((a, r) => a + (r.sent || 0), 0))}
    ${tile(
      "cron",
      !cronSeen ? "no run yet" : cronLate ? "late" : ago(cronAt),
      !cronSeen
        ? "runs at :17; nothing recorded since this was deployed"
        : cronLate
          ? '<span class="bad">nothing has run in over an hour, so the sequence is stopped</span>'
          : `<span class="ok">${esc(s.meta.last_cron?.v || "")}</span>`
    )}
  </div>
</section>

<section class="panel">
  <h2>Visitors</h2>
  ${dayChart(s.perDayViews, "visitors", "visitors")}
  <p class="muted small">Counted in the Worker, so this includes the people a beacon misses:
  blockers, anyone who left before a script would have run, and every reader of the docs from
  a terminal. Self-identifying crawlers are excluded, assets are not counted as pages, and
  nothing is stored that can be joined to a person: no IP, no cookie, and a per-day hash whose
  salt is deleted after two days. That is also why there is no returning-visitor figure. It
  cannot be computed, on purpose.</p>
  ${
    (tr.visitors || 0) === 0
      ? '<p class="muted small"><b>Nothing recorded yet.</b> Counting started at the deploy that added it, so this fills in from now rather than backwards.</p>'
      : `<p class="muted small">${tr.views || 0} views and ${tr.visitors || 0} visitors all time, since ${esc(tr.since || "")}.</p>`
  }
</section>

<section class="panel cols2">
  <div><h2>Pages</h2>${barList(s.paths, { empty: "No page views yet." })}</div>
  <div><h2>Came from</h2>${barList(s.referrers, { empty: "No referrers yet." })}</div>
</section>

<section class="panel cols2">
  <div><h2>Visitor countries</h2>${barList(s.visitorCountries, { empty: "No visitors yet." })}</div>
  <div>
    <h2>Device</h2>${barList(s.devices, { empty: "No visitors yet." })}
    <h2 style="margin-top:var(--k-space-3)">Landed on</h2>${barList(s.entries, { empty: "No visitors yet." })}
  </div>
</section>

<section class="panel">
  <h2>Signups</h2>
  ${dayChart(s.perDay)}
  <p class="muted small">First ${esc(t.first_at || "n/a")}, latest ${esc(t.last_at || "n/a")}.
  Confirmation takes ${secs(s.speed?.avg_s)} on average, worst ${secs(s.speed?.max_s)}.</p>
</section>

<section class="panel cols2">
  <div><h2>Signup countries</h2>${barList(s.countries, { empty: "No countries recorded." })}</div>
  <div><h2>Source</h2>${barList(s.sources, { empty: "No sources recorded." })}</div>
</section>

<section class="panel">
  <h2>The sequence</h2>
  <table><thead><tr><th>#</th><th>Day</th><th>Subject</th><th>Sent</th><th>Stuck</th><th>Delivered</th><th>Last error</th></tr></thead>
  <tbody>${s.sequence
    .map(
      (r) => `<tr>
    <td class="mono">${r.step}</td>
    <td class="mono">${r.after}</td>
    <td class="note">${esc(r.subject)}</td>
    <td class="mono">${r.sent || 0}${r.step === 0 ? " <span class='muted'>(inline)</span>" : ""}</td>
    <td class="mono ${r.stuck ? "bad" : ""}">${r.stuck || 0}</td>
    <td class="mono">${r.attempted ? pct(r.sent || 0, r.attempted) + "%" : "&mdash;"}</td>
    <td class="note ${r.last_error ? "bad" : "muted"}">${esc(r.last_error || "")}</td>
  </tr>`
    )
    .join("")}</tbody></table>
  <p class="muted small">Step 0 is the welcome and is sent by the signup handler, so it has no
  row of its own until a retry writes one. Stuck means claimed and not delivered; five failures
  and a step is left alone.</p>
</section>

<section class="panel cols2">
  <div>
    <h2>Where people leave</h2>
    ${barList(
      (s.churn || []).map((c) => ({ k: `after step ${c.k}`, n: c.n })),
      { empty: "Nobody has unsubscribed." }
    )}
    <p class="muted small">The last step someone was sent before they unsubscribed. A pile on
    one step is that email's problem, not the sequence's.</p>
  </div>
  <div>
    <h2>Delivery</h2>
    ${barList(s.byProvider, { empty: "Nothing sent yet." })}
    ${
      (s.failures || []).length
        ? `<table><thead><tr><th>Error</th><th>Count</th></tr></thead><tbody>${s.failures
            .map((f) => `<tr><td class="note bad">${esc(f.k)}</td><td class="mono">${f.n}</td></tr>`)
            .join("")}</tbody></table>`
        : '<p class="muted small">No send errors on record.</p>'
    }
  </div>
</section>

<section class="panel">
  <h2>Stripe</h2>
  ${
    (s.stripeTypes || []).length
      ? `<table><thead><tr><th>Event</th><th>Seen</th><th>Waiting</th><th>Live</th></tr></thead><tbody>${s.stripeTypes
          .map(
            (r) => `<tr><td class="mono">${esc(r.k)}</td><td class="mono">${r.n}</td>
        <td class="mono ${r.waiting ? "bad" : ""}">${r.waiting}</td>
        <td class="mono ${r.live ? "bad" : "muted"}">${r.live || 0}</td></tr>`
          )
          .join("")}</tbody></table>
      <p class="muted small">Waiting means received and not yet taken by the billing service.
      ${s.stripeOldest ? `Oldest undrained ${ago(s.stripeOldest)}. Stripe gives up retrying after about three days.` : ""}</p>`
      : '<p class="muted small">No Stripe events have arrived.</p>'
  }
</section>

<section class="panel">
  <h2>Health</h2>
  <table><tbody>
    <tr><td>Sender</td><td class="${s.sender ? "" : "bad"}">${s.sender || "nothing configured, so nothing can be sent"}</td></tr>
    <tr><td>Last cron</td><td class="${cronLate ? "bad" : ""}">${cronSeen ? ago(cronAt) : "not recorded yet"}${s.meta.last_cron ? ` &middot; ${esc(s.meta.last_cron.v)}` : ""}</td></tr>
    <tr><td>Last sequence run</td><td>${ago(s.meta.last_sequence?.at)}${
      s.meta.last_sequence ? ` &middot; ${esc(s.meta.last_sequence.v)}` : ""
    }</td></tr>
    <tr><td>Last retry sweep</td><td>${ago(s.meta.last_retry?.at)}${
      s.meta.last_retry ? ` &middot; ${esc(s.meta.last_retry.v)}` : ""
    }</td></tr>
  </tbody></table>
</section>`;
}

/* ------------------------------------------------------------- heartbeats */

export const SCHEMA = `
create table if not exists meta (
  k  text primary key,
  v  text not null,
  at text not null default (datetime('now'))
);
`;

/**
 * Record that something ran.
 *
 * Without this the admin can only show what is in the tables, and an empty table looks the
 * same whether the cron ran and had nothing to do or has not run since a bad deploy in March.
 */
export async function beat(env, k, v) {
  try {
    await env.kaviri_waitlist
      .prepare(
        "insert into meta (k, v, at) values (?, ?, datetime('now')) on conflict(k) do update set v = excluded.v, at = excluded.at"
      )
      .bind(k, String(v).slice(0, 200))
      .run();
  } catch {
    /* A missing heartbeat must never fail the work it was measuring. */
  }
}
