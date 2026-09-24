/*
 * Counting visitors, from the Worker, with no beacon and no cookie.
 *
 * The obvious move is Cloudflare Web Analytics or Plausible: a script tag, a third party, an
 * entry in the CSP, and a number that lives in someone else's dashboard where /admin cannot
 * reach it. This Worker already runs on every request to the site, which means the count is
 * already in front of us and the only question is whether we write it down.
 *
 * Doing it here also counts the people a beacon misses, which is the interesting half: anyone
 * with a blocker, anyone who left before the script ran, and every reader of the docs from a
 * terminal. A beacon measures the visitors who let you measure them.
 *
 * WHAT IS AND IS NOT STORED
 *
 * Not stored: the IP address, the full user agent, the query string, the path of anything
 * outside this site, or anything joinable to a person.
 *
 * Stored: a per-day count of views by path, and one row per visitor per day holding a hash,
 * their country, the site that sent them and whether they were on a phone.
 *
 * The hash is SHA-256 of a random salt, the IP and the user agent. The salt is generated on
 * the first request of each day and kept in `meta`; nothing derives it from the date, so it
 * cannot be recomputed by anyone who did not have it. Because it is new every day, the same
 * person on two days is two unrelated hashes and there is no way to follow anyone across
 * days, by us or by anyone who takes the database. That is deliberate and it is the reason
 * there is no "returning visitors" figure on the admin page: the design that makes the count
 * honest is the design that makes that number impossible, and a missing number is better than
 * a log that can be turned into a person.
 */

export const SCHEMA = `
create table if not exists views (
  d     text not null,
  path  text not null,
  n     integer not null default 0,
  primary key (d, path)
);
create table if not exists visits (
  d          text not null,
  vid        text not null,
  country    text,
  referrer   text,
  device     text,
  entry      text,
  n          integer not null default 1,
  primary key (d, vid)
);
create index if not exists visits_day on visits(d);
`;

/** Paths that are machinery rather than reading. Counting them flatters the numbers. */
const IGNORED = /^\/(admin|api|stripe|unsubscribe|favicon\.svg|robots\.txt)/;

/**
 * Crawlers, kept out of the human numbers.
 *
 * Deliberately not exhaustive, because it cannot be. It catches the ones that identify
 * themselves, which is most of the volume, and the rest quietly inflate the count. The
 * alternative is a fingerprinting arms race to make a vanity metric slightly less wrong.
 */
const BOT = /bot|crawler|spider|crawl|slurp|facebookexternalhit|preview|monitor|curl|wget|python-requests|headless|lighthouse|pingdom|uptime/i;

const PHONE = /iphone|android.*mobile|windows phone|ipod/i;
const TABLET = /ipad|android(?!.*mobile)|tablet/i;

function device(ua) {
  if (PHONE.test(ua)) return "phone";
  if (TABLET.test(ua)) return "tablet";
  return "desktop";
}

/**
 * The sending site, as a hostname and nothing else.
 *
 * A full referrer URL is somebody's search terms, or the private page they had open. The
 * hostname answers "where did they come from" and carries none of that.
 */
function referrer(raw, self) {
  if (!raw) return "direct";
  try {
    const h = new URL(raw).hostname.replace(/^www\./, "");
    return h === self ? "direct" : h.slice(0, 80);
  } catch {
    return "direct";
  }
}

/** Today's salt, made once and kept. See the note at the top for why it is not derived. */
async function saltFor(db, day) {
  const key = `salt:${day}`;
  const found = await db.prepare("select v from meta where k = ?").bind(key).first();
  if (found?.v) return found.v;
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  const salt = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  // Two requests in the same second both make one; the first to land wins and the other
  // reads it back, so a handful of visitors on a day boundary can never be double counted.
  await db
    .prepare("insert into meta (k, v, at) values (?, ?, datetime('now')) on conflict(k) do nothing")
    .bind(key, salt)
    .run();
  const now = await db.prepare("select v from meta where k = ?").bind(key).first();
  return now?.v || salt;
}

async function hash(s) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("").slice(0, 32);
}

/**
 * Record one request. Called from `ctx.waitUntil`, so it never delays a response.
 *
 * Everything is swallowed. A page that fails to serve because the analytics write failed is a
 * page that has traded the product for the measurement of the product.
 */
export async function record(request, env, url) {
  try {
    if (request.method !== "GET" && request.method !== "HEAD") return;
    if (IGNORED.test(url.pathname)) return;

    const ua = request.headers.get("user-agent") || "";
    if (BOT.test(ua)) return;

    // Only pages. An HTML page view is a visit; the stylesheet and the video that come with
    // it are not four more.
    const p = url.pathname;
    const isPage = p.endsWith("/") || !/\.[a-z0-9]{2,5}$/i.test(p);
    if (!isPage) return;

    const db = env.kaviri_waitlist;
    const day = new Date().toISOString().slice(0, 10);
    const path = p.length > 120 ? p.slice(0, 120) : p;

    const ip = request.headers.get("cf-connecting-ip") || "";

    /*
     * Create the tables if the write says they are not there, then write again.
     *
     * The schema is otherwise applied by the cron and by /admin, and neither of those is on
     * the path a visitor takes. On the deploy that added this, every visitor between the
     * deploy and the first time somebody opened the admin page was silently dropped: record()
     * swallows its errors by design, so the counter read zero and looked like no traffic
     * rather than like a missing table.
     */
    const write = async () => {
      const salt = await saltFor(db, day);
      const vid = await hash(`${salt}|${ip}|${ua}`);
      return db.batch([
        db
          .prepare("insert into views (d, path, n) values (?, ?, 1) on conflict(d, path) do update set n = n + 1")
          .bind(day, path),
        db
          .prepare(
            `insert into visits (d, vid, country, referrer, device, entry, n)
                  values (?, ?, ?, ?, ?, ?, 1)
             on conflict(d, vid) do update set n = n + 1`
          )
          .bind(
            day,
            vid,
            request.cf?.country || null,
            referrer(request.headers.get("referer"), url.hostname.replace(/^www\./, "")),
            device(ua),
            path
          ),
      ]);
    };

    try {
      await write();
    } catch {
      const { ensureSchemaOnce } = await import("./sequence.js");
      await ensureSchemaOnce(env);
      await write();
    }
  } catch {
    /* Never the reason a page did not load. */
  }
}

/**
 * Drop the day salts once they are old enough that nobody could still be matched by them.
 *
 * Keeping them forever would mean that anyone who took the database and had a suspect's IP
 * and user agent could confirm whether that person visited, on any day on record. Deleting
 * the salt makes the hashes from that day permanently unlinkable to anyone, which is the
 * whole point of salting them in the first place.
 */
export async function forgetOldSalts(env, keepDays = 2) {
  try {
    await env.kaviri_waitlist
      .prepare("delete from meta where k like 'salt:%' and substr(k, 6) < date('now', ?)")
      .bind(`-${keepDays} days`)
      .run();
  } catch {
    /* Housekeeping, not a job worth failing a cron over. */
  }
}

/**
 * Keep the visit table bounded.
 *
 * `views` has a row per day per path and cannot run away. `visits` has a row per visitor per
 * day, so it grows with however well the site does, which is the one table here that could
 * quietly become the reason a free D1 fills up. A year is far more history than anything on
 * /admin reads, and the all-time figures name the date they start from, so they stay true as
 * the window slides rather than silently becoming a different number.
 */
export async function pruneVisits(env, keepDays = 400) {
  try {
    await env.kaviri_waitlist
      .prepare("delete from visits where d < date('now', ?)")
      .bind(`-${keepDays} days`)
      .run();
  } catch {
    /* As above. */
  }
}
