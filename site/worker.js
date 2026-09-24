/**
 * kaviri.dev.
 *
 * A Worker in front of Cloudflare's static asset store. It exists for three
 * things the asset store will not do on its own: pick a Cache-Control per kind
 * of file, attach security headers, and keep its own source out of the public
 * tree. Everything else is a straight pass-through, including Range requests,
 * conditional requests and 304s.
 */

// Files that sit in the asset directory because they have to be next to the
// page, but that must never be served. wrangler.toml carries the account ID.
import { keepDatabaseAwake, retryUnconfirmed, signup } from "./wsrc/waitlist.js";
import { ensureSchema, runSequence, unsubscribe } from "./wsrc/sequence.js";
import * as admin from "./wsrc/admin.js";
import * as stripe from "./wsrc/stripe.js";

const PRIVATE_PATHS = new Set(["/worker.js", "/wrangler.toml", "/.assetsignore"]);

// A build step that fingerprints a file puts the hash in the name, which is the
// only honest signal that a URL's bytes can never change. Both common spellings
// are matched: style.a1b2c3d4.css and style-a1b2c3d4.css.
const HASHED_NAME = /[.-][0-9a-f]{8,32}\.[a-z0-9]+$/i;

const YEAR = 31536000;
const MONTH = 2592000;

/**
 * The page loads no third-party script, font or stylesheet, so the policy can
 * be closed almost all the way. Each entry earns its place:
 *
 * - default-src 'self' is the floor; anything not named below falls back to it.
 * - script-src 'self' with no 'unsafe-inline': an injected <script> in the page
 *   would not run, which is the whole point of having a CSP on a static site.
 *   Adding an inline script later means adding a hash here, not 'unsafe-inline'.
 * - style-src allows 'unsafe-inline' because a one-page site keeps its CSS in a
 *   <style> block and in style attributes. This is the one loosened directive,
 *   and it is loose only for styles, which cannot exfiltrate on their own.
 * - img-src allows data: for inline SVG and favicons encoded into the HTML.
 * - media-src 'self' is what lets the demo MP4 play. Drop it and the <video>
 *   element goes black with no console error that names the cause.
 * - frame-ancestors 'self' stops another site framing the page, which is the modern
 *   X-Frame-Options and the reason this file does not send that header too.
 * - base-uri 'none' stops an injected <base> silently repointing every relative
 *   URL on the page, including the video.
 * - form-action 'none' is safe today because the page has no form. A signup
 *   form later needs this changed, or the submit does nothing.
 * - upgrade-insecure-requests catches a hand-written http:// asset URL rather
 *   than letting it become a mixed-content block.
 */
const CSP = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data:",
  "media-src 'self'",
  "font-src 'self'",
  "connect-src 'self'",
  "object-src 'none'",
  /*
   * 'self', not 'none'. The playground frames a bundled app from this same origin so the
   * camera has something to move over, and 'none' blocks that too: the browser refuses the
   * frame and the runner sees a null contentDocument with no console error worth the name.
   * Against clickjacking the two are equivalent, because the attack needs a DIFFERENT origin
   * to do the framing, and every origin but this one is still refused.
   */
  "frame-ancestors 'self'",
  "base-uri 'none'",
    /*
   * 'self', not 'none'. The admin's login is a real form post to this origin, and 'none'
   * refuses it before it leaves the page. The value that matters is still there: a form on
   * this site cannot be made to submit anywhere else.
   */
  "form-action 'self'",
  "upgrade-insecure-requests",
].join("; ");

/**
 * Deny the sensors and capabilities a landing page has no use for, so that an
 * injected script cannot reach them even if it somehow runs.
 *
 * fullscreen and autoplay are granted to 'self' on purpose. The demo video is
 * the reason this site exists: revoking fullscreen greys out the button in the
 * native video controls, and revoking autoplay stops a muted inline preview
 * from ever starting in Chrome.
 */
const PERMISSIONS_POLICY = [
  "accelerometer=()",
  "camera=()",
  /*
   * The playground records its own preview and hands the viewer a file, which needs tab
   * capture. 'self' rather than '*': this page may ask, anything this page embeds may not,
   * and the browser still shows its own picker and its own recording indicator, so nothing is
   * captured without the person choosing it in an interface that is not ours.
   */
  "display-capture=(self)",
  "geolocation=()",
  "gyroscope=()",
  "magnetometer=()",
  "microphone=()",
  "midi=()",
  "payment=()",
  "usb=()",
  "autoplay=(self)",
  "fullscreen=(self)",
].join(", ");

/** Decide the Cache-Control for one asset. */
function cacheControl(pathname, contentType) {
  const type = (contentType || "").split(";")[0].trim().toLowerCase();

  // A fingerprinted URL cannot change meaning, so it is cached for a year and
  // marked immutable. Without immutable, Safari and Firefox still revalidate on
  // a reload, which is a round trip per asset for a file that by construction
  // cannot have changed.
  if (HASHED_NAME.test(pathname)) {
    return `public, max-age=${YEAR}, immutable`;
  }

  // HTML is the deploy's visible edge. A short max-age means a fix is live in
  // about a minute; stale-while-revalidate means a burst of traffic is answered
  // from cache while exactly one request goes back to revalidate, instead of
  // every visitor arriving at the origin the second the TTL lapses.
  // stale-if-error keeps the page up through an origin failure rather than
  // showing Cloudflare's error card.
  if (type === "text/html") {
    return "public, max-age=60, stale-while-revalidate=86400, stale-if-error=86400";
  }

  // The demo video is large and is replaced only when the product changes, so
  // it gets a month rather than a year: long enough that a repeat visitor never
  // refetches it, short enough that a rename is not required to ship a new cut.
  // It is not marked immutable, because the filename carries no hash.
  if (type.startsWith("video/") || type.startsWith("audio/")) {
    return `public, max-age=${MONTH}, stale-while-revalidate=86400`;
  }

  // Fonts are effectively permanent even without a hash in the name; a font is
  // replaced by adding a new one, not by editing the old one in place.
  if (type.startsWith("font/") || pathname.endsWith(".woff2")) {
    return `public, max-age=${YEAR}, immutable`;
  }

  // Unhashed CSS, JS, images and everything else. An hour of browser cache with
  // a day of stale-while-revalidate behind it: a deploy reaches everyone within
  // the hour without a hard refresh, and nothing here is worth an origin hit.
  return "public, max-age=3600, stale-while-revalidate=86400";
}

function securityHeaders(headers) {
  headers.set("Content-Security-Policy", CSP);

  // Without nosniff, a file served as text/plain that happens to start with
  // markup can be sniffed into HTML and executed in this origin.
  headers.set("X-Content-Type-Options", "nosniff");

  // Send the full URL to ourselves and only the origin to anyone else, so an
  // outbound link never leaks the path someone was reading, and never sends a
  // referrer at all when downgrading to HTTP.
  headers.set("Referrer-Policy", "strict-origin-when-cross-origin");

  headers.set("Permissions-Policy", PERMISSIONS_POLICY);

  // A year of HTTPS-only, subdomains included. This is safe only because the
  // zone has Always Use HTTPS on. preload is deliberately absent: it is a
  // one-way door enforced by browser vendors, not by us.
  headers.set("Strict-Transport-Security", `max-age=${YEAR}; includeSubDomains`);

  // Isolate the browsing context from anything that opens it or that it opens,
  // so a window.opener from another site cannot reach into this page.
  headers.set("Cross-Origin-Opener-Policy", "same-origin");

  // The site embeds nothing from elsewhere and nobody should be hotlinking the
  // demo video into their own page.
  headers.set("Cross-Origin-Resource-Policy", "same-origin");

  // Cloudflare's asset store already reports its own server identity; there is
  // nothing to gain from also announcing the framework.
  headers.delete("X-Powered-By");

  return headers;
}

/**
 * The security headers belong on everything this Worker answers, not only on the files it
 * serves from the asset store. A 303 out of the admin login with no CSP on it is still a
 * response from this origin.
 */
function withSecurity(response) {
  const out = new Response(response.body, response);
  securityHeaders(out.headers);
  return out;
}

export default {
  /*
   * Anything whose confirmation could not be sent is retried here rather than being lost. It
   * is the difference between "the mail provider was down for ten minutes" and "those four
   * people never heard back".
   */
  async scheduled(event, env, ctx) {
    /*
     * Order matters and this is deliberately one awaited chain rather than three waitUntils.
     * ensureSchema creates the table runSequence reads, and retryUnconfirmed can move somebody
     * to confirmed, which is what makes them eligible for step 1 in the same run.
     */
    ctx.waitUntil(
      (async () => {
        await ensureSchema(env).catch(() => {});
        await retryUnconfirmed(env, 50).catch(() => {});
        await runSequence(env).catch(() => {});
      })()
    );
    // A free Supabase project pauses after a week idle, and a paused project is a dead
    // service with no warning. One cheap request an hour is the whole defence.
    ctx.waitUntil(keepDatabaseAwake(env));
  },

  async fetch(request, env, ctx) {
    const url = new URL(request.url);

    /*
     * Three things here are not the static site: the waitlist endpoint, the admin surface and
     * the scheduled retry. They are handled before the method gate and before the asset store,
     * because they are the only paths that accept a POST.
     */
    if (url.pathname === "/api/waitlist" && request.method === "POST") {
      return withSecurity(await signup(request, env, ctx));
    }
    /*
     * Stripe delivers here because that is the URL configured on the endpoint. It only
     * verifies and stores; kaviri-billing is what reads the stored events and decides what
     * they mean. See wsrc/stripe.js for why a buffer rather than a 404.
     */
    if (url.pathname === "/stripe" && request.method === "POST") {
      return withSecurity(await stripe.receive(request, env));
    }
    if (url.pathname === "/admin" || url.pathname.startsWith("/admin/")) {
      return withSecurity(await admin.handle(request, env, url));
    }
    /*
     * Unsubscribe takes both verbs. GET is the link at the bottom of the mail; POST is what
     * Gmail and Apple Mail send for their own one-click control, and it must work without a
     * confirmation page or they do not show the control at all. Both are before the method
     * gate because the gate allows only GET and HEAD.
     */
    if (url.pathname === "/unsubscribe" && (request.method === "GET" || request.method === "POST")) {
      return withSecurity(await unsubscribe(request, env, url));
    }

    // The rest of the site is read-only. Answering anything else with a clear 405 is more
    // useful than letting a POST fall through to the asset store's 404, which
    // reads like the page is missing rather than like the method is wrong.
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed.\n", {
        status: 405,
        headers: securityHeaders(
          new Headers({
            Allow: "GET, HEAD",
            "Content-Type": "text/plain; charset=utf-8",
            "Cache-Control": "no-store",
          })
        ),
      });
    }

    // One canonical hostname, so links, analytics and the HTTP cache all agree
    // on one URL per page. 301 rather than 302 because this will not change.
    if (url.hostname === "www.kaviri.dev") {
      url.hostname = "kaviri.dev";
      return new Response(null, {
        status: 301,
        headers: securityHeaders(
          new Headers({
            Location: url.toString(),
            "Cache-Control": `public, max-age=${MONTH}`,
          })
        ),
      });
    }

    // Deployment metadata lives in this directory and must not be reachable.
    // 404 rather than 403, because a 403 confirms the file is there.
    if (PRIVATE_PATHS.has(url.pathname)) {
      return new Response("Not found.\n", {
        status: 404,
        headers: securityHeaders(
          new Headers({
            "Content-Type": "text/plain; charset=utf-8",
            "Cache-Control": "no-store",
          })
        ),
      });
    }

    // The original request is passed through untouched, which is what carries
    // Range, If-None-Match and If-Modified-Since to the asset store. Safari
    // will not start an MP4 at all unless the first ranged request comes back
    // as a 206 with Content-Range, so this must not be rebuilt as a plain GET.
    const asset = await env.ASSETS.fetch(request);

    // Rebuilding from asset.body preserves the status, so a 206 stays a 206, a
    // 304 stays a 304 with a null body, and Content-Range and Content-Length
    // survive. Reading the body here instead would break ranged playback.
    const response = new Response(asset.body, asset);
    const headers = securityHeaders(response.headers);
    // response.headers is the live header map of the object being returned, so
    // everything below mutates the response in place. Building a second
    // Response from response.body would work too, but it would put a second
    // stream hop in front of every byte of the video for no gain.

    // Without this a browser has no way to know ranged requests are allowed, so
    // it fetches the whole video before playing and seeking does nothing. The
    // asset store normally sets it; setting it again is harmless and means one
    // less thing that depends on the asset store's defaults.
    headers.set("Accept-Ranges", "bytes");

    if (response.status === 404) {
      // The 404 page is generated from the same deploy as the site, so it can
      // be cached briefly, but never as long as a real page.
      headers.set("Cache-Control", "public, max-age=60");
    } else {
      headers.set("Cache-Control", cacheControl(url.pathname, headers.get("Content-Type")));
    }

    return response;
  },
};
