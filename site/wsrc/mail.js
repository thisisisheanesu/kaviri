/*
 * Sending mail from a Worker.
 *
 * There is no good option here, so there are two and the first one configured wins.
 *
 * Cloudflare's own send_email binding is deliberately not one of them: it can only deliver to
 * an address already verified on the account, which is fine for telling the owner about a
 * signup and useless for telling the person who signed up that they are on the list.
 *
 * 1. Resend, if RESEND_API_KEY is set. One HTTPS call, a real sending domain, DKIM, bounce
 *    handling, a dashboard. This is what should be used once the domain is verified.
 * 2. SMTP over a raw TCP socket, if SMTP_HOST and friends are set. Kept, and off by default,
 *    because MEASURED ON THIS ACCOUNT IT DOES NOT WORK: Cloudflare blocks outbound connections
 *    to the submission ports from Workers, and the block is a black hole rather than a refusal,
 *    so connect() resolves, the first read never returns, and the whole thing hangs until the
 *    request is killed with nothing written anywhere. That is why sendMail has a deadline. If
 *    you are reading this because you are about to try SMTP from a Worker again: it was tried,
 *    on 23 September 2026, against smtp.gmail.com:587, and this is what happened.
 *
 * If neither is configured the caller is told so rather than being told the mail was sent. A
 * waitlist that silently drops its confirmations is worse than one that admits it.
 */

import { connect } from "cloudflare:sockets";

export function senderName(env) {
  if (env.RESEND_API_KEY) return "resend";
  if (env.SMTP_ENABLED === "1" && env.SMTP_HOST && env.SMTP_USER && env.SMTP_PASS) return "smtp";
  return null;
}

/**
 * Fifteen seconds, then it failed.
 *
 * Without this a sender that hangs takes the whole confirmation with it: the update that
 * records the failure is after the send, so a hang leaves the row looking untried rather than
 * failed, and the hourly retry then retries something that will hang again. A deadline turns a
 * hang into an ordinary error with a message in it.
 */
const DEADLINE_MS = 15000;

function withDeadline(promise, via) {
  let timer;
  const bell = new Promise((resolve) => {
    timer = setTimeout(() => resolve({ sent: false, via, error: `${via} timed out after ${DEADLINE_MS}ms` }), DEADLINE_MS);
  });
  return Promise.race([promise, bell]).finally(() => clearTimeout(timer));
}

/**
 * Send one email.
 *
 * `text` is required and `html` is optional, never the other way round. A multipart message
 * with a real plain-text alternative is what a terminal client, a screen reader and most spam
 * filters actually read, and an HTML-only mail from a new domain is the single easiest way to
 * land in a spam folder.
 *
 * `listUnsubscribe` turns on the header pair that makes Gmail and Apple Mail show their own
 * unsubscribe control next to the sender. Offering it is what stops a reader who wants out
 * from using the report-spam button instead, which costs the whole domain's reputation rather
 * than one subscriber.
 */
export async function sendMail(env, { to, subject, text, html, replyTo, listUnsubscribe }) {
  const from = env.MAIL_FROM || "kaviri <hello@kaviri.dev>";
  const which = senderName(env);
  if (!which) return { sent: false, via: null, error: "no sender configured" };
  try {
    const work =
      which === "resend"
        ? viaResend(env, { from, to, subject, text, html, replyTo, listUnsubscribe })
        : viaSmtp(env, { from, to, subject, text, html, replyTo, listUnsubscribe });
    return await withDeadline(work, which);
  } catch (e) {
    return { sent: false, via: which, error: String(e && e.message ? e.message : e) };
  }
}

/** The two headers, or nothing. Half of the pair is worse than neither: One-Click without a URL. */
function unsubHeaders(listUnsubscribe) {
  if (!listUnsubscribe) return {};
  return {
    "List-Unsubscribe": `<${listUnsubscribe}>`,
    "List-Unsubscribe-Post": "List-Unsubscribe=One-Click",
  };
}

async function viaResend(env, { from, to, subject, text, html, replyTo, listUnsubscribe }) {
  const r = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${env.RESEND_API_KEY}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      from,
      to: [to],
      subject,
      text,
      ...(html ? { html } : {}),
      reply_to: replyTo,
      ...(listUnsubscribe ? { headers: unsubHeaders(listUnsubscribe) } : {}),
    }),
  });
  if (!r.ok) return { sent: false, via: "resend", error: `${r.status} ${(await r.text()).slice(0, 200)}` };
  return { sent: true, via: "resend" };
}

/* ------------------------------------------------------------------- SMTP */

/**
 * A reader that yields whole SMTP replies.
 *
 * A reply is one or more lines; every line but the last has a hyphen after the code, as in
 * `250-SIZE` then `250 HELP`. Reading a fixed number of chunks would work until the day a
 * server splits a multiline greeting across two packets, which is the kind of bug that only
 * shows up in production.
 */
function replyReader(readable) {
  const reader = readable.getReader();
  const dec = new TextDecoder();
  let buf = "";
  return {
    async next() {
      for (;;) {
        const m = buf.match(/^(?:\d{3}-[^\n]*\n)*(\d{3}) [^\n]*\r?\n/);
        if (m) {
          const whole = m[0];
          buf = buf.slice(whole.length);
          return { code: Number(m[1]), text: whole.trim() };
        }
        const { value, done } = await reader.read();
        if (done) throw new Error(`connection closed mid-reply: ${buf.slice(0, 120)}`);
        buf += dec.decode(value, { stream: true });
      }
    },
    release() {
      try { reader.releaseLock(); } catch { /* the socket is going away anyway */ }
    },
  };
}

function b64(s) {
  const bytes = new TextEncoder().encode(s);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

/** A header value cannot contain a newline, or the rest of it becomes new headers. */
function header(v) {
  return String(v).replace(/[\r\n]+/g, " ").trim();
}

async function viaSmtp(env, { from, to, subject, text, html, replyTo, listUnsubscribe }) {
  const port = Number(env.SMTP_PORT || 587);
  const socket = connect({ hostname: env.SMTP_HOST, port }, { secureTransport: "starttls" });

  let sock = socket;
  let writer = sock.writable.getWriter();
  let rd = replyReader(sock.readable);
  const enc = new TextEncoder();

  const say = async (line, expect) => {
    if (line !== null) await writer.write(enc.encode(line + "\r\n"));
    const r = await rd.next();
    if (expect && !expect.includes(r.code)) {
      throw new Error(`smtp ${line === null ? "greeting" : line.split(" ")[0]}: ${r.text.slice(0, 160)}`);
    }
    return r;
  };

  try {
    await say(null, [220]);
    await say("EHLO kaviri.dev", [250]);
    await say("STARTTLS", [220]);

    // Everything after this point is on the upgraded socket, so the old writer and reader are
    // finished with and their locks have to go before the new ones can be taken.
    await writer.close().catch(() => {});
    rd.release();
    sock = sock.startTls();
    writer = sock.writable.getWriter();
    rd = replyReader(sock.readable);

    await say("EHLO kaviri.dev", [250]);
    await say("AUTH LOGIN", [334]);
    await say(b64(env.SMTP_USER), [334]);
    await say(b64(env.SMTP_PASS), [235]);

    const envelopeFrom = (from.match(/<([^>]+)>/) || [null, from])[1];
    await say(`MAIL FROM:<${envelopeFrom}>`, [250]);
    await say(`RCPT TO:<${to}>`, [250, 251]);
    await say("DATA", [354]);

    // A lone dot on a line ends DATA, so any line that is just a dot gets one more.
    const stuff = (s) => String(s).replace(/\r?\n/g, "\r\n").replace(/^\.$/gm, "..");

    /*
     * multipart/alternative when there is HTML, and the plain part goes FIRST. The order is
     * the spec's way of saying which part is preferred: a client picks the last one it can
     * render, so text before html means an HTML client shows the HTML and a plain one shows
     * the text. Reversed, everybody gets plain text.
     */
    const bound = `kv-${crypto.randomUUID()}`;
    const unsub = unsubHeaders(listUnsubscribe);

    const head = [
      `From: ${header(from)}`,
      `To: ${header(to)}`,
      replyTo ? `Reply-To: ${header(replyTo)}` : null,
      `Subject: ${header(subject)}`,
      ...Object.entries(unsub).map(([k, v]) => `${k}: ${header(v)}`),
      "MIME-Version: 1.0",
    ].filter((l) => l !== null);

    const body = (
      html
        ? [
            ...head,
            `Content-Type: multipart/alternative; boundary="${bound}"`,
            "",
            `--${bound}`,
            'Content-Type: text/plain; charset="utf-8"',
            "Content-Transfer-Encoding: 8bit",
            "",
            stuff(text),
            `--${bound}`,
            'Content-Type: text/html; charset="utf-8"',
            "Content-Transfer-Encoding: 8bit",
            "",
            stuff(html),
            `--${bound}--`,
            ".",
          ]
        : [
            ...head,
            'Content-Type: text/plain; charset="utf-8"',
            "Content-Transfer-Encoding: 8bit",
            "",
            stuff(text),
            ".",
          ]
    ).join("\r\n");

    await writer.write(enc.encode(body + "\r\n"));
    const done = await rd.next();
    if (done.code !== 250) throw new Error(`smtp DATA: ${done.text.slice(0, 160)}`);

    await say("QUIT", [221]).catch(() => {});
    return { sent: true, via: "smtp" };
  } finally {
    try { await writer.close(); } catch { /* already gone */ }
    try { await sock.close(); } catch { /* already gone */ }
  }
}

/* --------------------------------------------------- the owner's own mailbox */

/**
 * Tell the owner something happened, through Cloudflare's send_email binding.
 *
 * Separate from sendMail on purpose. This one can only ever reach the single verified
 * destination, needs no third party and no key, and therefore keeps working on the day the
 * real sender does not. Losing a confirmation is bad; not knowing anyone signed up is worse.
 */
export async function notifyOwner(env, { subject, text }) {
  if (!env.OWNER_MAIL || !env.OWNER_EMAIL) return { sent: false, error: "no owner binding" };
  const { EmailMessage } = await import("cloudflare:email");
  const from = "waitlist@kaviri.dev";
  const raw = [
    `From: kaviri <${from}>`,
    `To: ${env.OWNER_EMAIL}`,
    `Subject: ${String(subject).replace(/[\r\n]+/g, " ")}`,
    `Message-ID: <${crypto.randomUUID()}@kaviri.dev>`,
    "MIME-Version: 1.0",
    'Content-Type: text/plain; charset="utf-8"',
    "",
    String(text).replace(/\r?\n/g, "\r\n"),
  ].join("\r\n");
  try {
    await env.OWNER_MAIL.send(new EmailMessage(from, env.OWNER_EMAIL, raw));
    return { sent: true };
  } catch (e) {
    return { sent: false, error: String(e && e.message ? e.message : e) };
  }
}
