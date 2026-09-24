/*
 * The Stripe webhook receiver.
 *
 * This is deliberately the dumbest thing that can be correct. It verifies Stripe's signature
 * and writes the event to D1. It does not know what a subscription is, it does not touch an
 * entitlement, and it never will: that is kaviri-billing's job, it is closed source, and the
 * seam between them is the whole design.
 *
 * It exists because the alternative is losing events. The billing service is not deployed
 * anywhere yet, the webhook endpoint in Stripe points here, and Stripe gives up retrying after
 * about three days. Without something on this end, every subscription event between now and
 * the day billing is deployed is gone, and the first symptom would be a paying customer whose
 * entitlement never changed.
 *
 * So: a durable buffer. Stripe delivers, this stores the raw bytes, and the billing service
 * drains the table whenever it comes up. Nothing is interpreted in between, which is what
 * keeps this out of the seam.
 */

export const SCHEMA = `
create table if not exists stripe_events (
  -- Stripe's own event id. The primary key is what makes redelivery a no-op: Stripe retries
  -- the same id, and an insert-or-ignore is the whole idempotency story.
  id           text primary key,
  type         text not null,
  created      integer not null,
  livemode     integer not null,
  payload      text not null,
  received_at  text not null default (datetime('now')),
  -- Set by the billing service when it has taken this event. Null means still owed.
  drained_at   text
);
create index if not exists stripe_events_undrained on stripe_events(created) where drained_at is null;
`;

/** Stripe's tolerance, and the reason a replayed capture from last week is not accepted. */
const TOLERANCE_S = 300;

const enc = new TextEncoder();

function hex(buf) {
  return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Compare without leaking where two signatures first differ. */
function sameHex(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

/**
 * Verify a `Stripe-Signature` header against the raw body.
 *
 * The raw bytes, never a re-serialised object: Stripe signs exactly what it sent, and parsing
 * then re-encoding changes key order and whitespace, which turns into a signature failure that
 * looks intermittent because it depends on the payload.
 */
async function verify(secret, header, raw) {
  const parts = Object.fromEntries(
    String(header || "")
      .split(",")
      .map((p) => p.trim().split("="))
      .filter((p) => p.length === 2),
  );
  const t = Number(parts.t);
  if (!t || !parts.v1) return "malformed Stripe-Signature header";

  const age = Math.abs(Math.floor(Date.now() / 1000) - t);
  if (age > TOLERANCE_S) return `timestamp is ${age}s away, outside the ${TOLERANCE_S}s tolerance`;

  const key = await crypto.subtle.importKey(
    "raw",
    enc.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = hex(await crypto.subtle.sign("HMAC", key, enc.encode(`${t}.${raw}`)));

  // The header can carry several v1 signatures during a secret rotation. Any one matching is
  // a pass, which is what makes rolling the signing secret a non-event.
  const given = String(header)
    .split(",")
    .map((p) => p.trim())
    .filter((p) => p.startsWith("v1="))
    .map((p) => p.slice(3));
  return given.some((g) => sameHex(g, mac)) ? null : "no v1 signature matched";
}

export async function receive(request, env) {
  const json = (body, status) =>
    new Response(JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store" },
    });

  if (!env.STRIPE_WEBHOOK_SECRET) {
    // 503 rather than 200. A 200 would tell Stripe the event was handled and it would never
    // retry, so a misconfiguration here would silently eat events instead of queueing them.
    return json({ error: "no STRIPE_WEBHOOK_SECRET configured" }, 503);
  }

  const raw = await request.text();
  const why = await verify(env.STRIPE_WEBHOOK_SECRET, request.headers.get("stripe-signature"), raw);
  if (why) {
    // 400 and no retry. A signature failure is not transient, and asking Stripe to resend
    // something that will be rejected identically just fills its queue.
    return json({ error: `signature verification failed: ${why}` }, 400);
  }

  let event;
  try {
    event = JSON.parse(raw);
  } catch {
    return json({ error: "signed body was not JSON" }, 400);
  }
  if (!event?.id || !event?.type) return json({ error: "not a Stripe event" }, 400);

  await env.kaviri_waitlist
    .prepare(
      "insert or ignore into stripe_events (id, type, created, livemode, payload) values (?, ?, ?, ?, ?)",
    )
    .bind(event.id, event.type, Number(event.created) || 0, event.livemode ? 1 : 0, raw)
    .run();

  return json({ received: true, id: event.id, type: event.type, stored: true }, 200);
}

/** What the billing service drains, and what /admin shows. */
export async function pending(env, limit = 100) {
  const { results } = await env.kaviri_waitlist
    .prepare(
      "select id, type, created, livemode, received_at from stripe_events where drained_at is null order by created limit ?",
    )
    .bind(limit)
    .all();
  return results || [];
}
