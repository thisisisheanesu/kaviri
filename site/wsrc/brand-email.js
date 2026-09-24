/*
 * The branded email shell.
 *
 * Every mail kaviri sends to a person goes through render(). One place decides what the
 * letterhead looks like, so changing it changes all of them, including the ones already
 * queued for a retry.
 *
 * Three rules this file follows that are not the rules the website follows:
 *
 * 1. RAW HEX IS CORRECT HERE. brand/tokens.css says hex outside that file is a bug, and for
 *    the site it is. Email clients do not support custom properties, Gmail strips <style>
 *    entirely, and Outlook resolves var() to nothing and paints black on black. The values
 *    below are copied from tokens.css by hand and the pairs are listed in TOKENS so a change
 *    there can be found here. There is no build step to do it for us.
 * 2. Layout is tables and inline styles. Not nostalgia: Gmail on Android still drops a flex
 *    container's children into a single column, and Outlook on Windows renders through Word.
 * 3. No remote images, no web fonts. An image-blocked email should look finished rather than
 *    broken, so the wordmark is text. It also keeps the mail out of the bucket where a
 *    tracking pixel would put it.
 *
 * Dark mode is offered through prefers-color-scheme in a <style> block. Apple Mail and iOS
 * honour it; Gmail ignores it and gets the light version, which is why the light version has
 * to be the one that is right.
 */

/** tokens.css name -> the literal this file uses. Keep them in step by hand. */
const TOKENS = {
  "--kv-ink": "#0f0f10",
  "--kv-n-50": "#f7f7f5",
  "--kv-n-100": "#ededea",
  "--kv-n-200": "#dfdfdb",
  "--kv-n-400": "#8a8a85",
  "--kv-n-500": "#5a5a57",
  "--kv-accent-500": "#c7361a",
  "--kv-accent-wash": "#f7e9e3",
  "--kv-white": "#ffffff",
};

const C = {
  ink: TOKENS["--kv-ink"],
  paper: TOKENS["--kv-white"],
  page: TOKENS["--kv-n-50"],
  rule: TOKENS["--kv-n-200"],
  wash: TOKENS["--kv-n-100"],
  muted: TOKENS["--kv-n-500"],
  faint: TOKENS["--kv-n-400"],
  accent: TOKENS["--kv-accent-500"],
  accentWash: TOKENS["--kv-accent-wash"],
};

const SANS = "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif";
const MONO = "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";

export function esc(s) {
  return String(s ?? "").replace(/[<>&"]/g, (c) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;", '"': "&quot;" })[c]);
}

/* ------------------------------------------------------------------ blocks */

/*
 * A body is an array of blocks rather than a string of HTML, so a step in the sequence
 * describes what it wants to say and never has to get a table nested correctly. Each block
 * knows how to render itself twice: once as HTML and once as the plain-text alternative,
 * which is what keeps the two versions from drifting apart.
 */

export const p = (text) => ({ kind: "p", text });
export const lead = (text) => ({ kind: "lead", text });
export const h = (text) => ({ kind: "h", text });
export const code = (text) => ({ kind: "code", text });
export const button = (label, href) => ({ kind: "button", label, href });
export const link = (label, href) => ({ kind: "link", label, href });
export const rule = () => ({ kind: "rule" });
export const note = (text) => ({ kind: "note", text });

/**
 * Inline markup, kept to the two things prose actually needs.
 *
 * `code` for a literal and *emphasis* for a word. Anything more and a step's copy becomes a
 * template language, which is the road to an email nobody can read in the source.
 */
function inline(text) {
  return esc(text)
    .replace(/`([^`]+)`/g, `<code style="font-family:${MONO};font-size:13px;background:${C.wash};padding:1px 4px;border-radius:3px;">$1</code>`)
    .replace(/\*([^*]+)\*/g, '<em style="font-style:normal;font-weight:600;">$1</em>')
    .replace(/\n/g, "<br>");
}

/** The same markers, removed rather than rendered. */
function plain(text) {
  return String(text).replace(/`([^`]+)`/g, "$1").replace(/\*([^*]+)\*/g, "$1");
}

function blockHtml(b) {
  const pStyle = `margin:0 0 16px;font-family:${SANS};font-size:15px;line-height:1.6;color:${C.ink};`;
  switch (b.kind) {
    case "lead":
      return `<p style="margin:0 0 20px;font-family:${SANS};font-size:17px;line-height:1.5;color:${C.ink};">${inline(b.text)}</p>`;
    case "p":
      return `<p style="${pStyle}">${inline(b.text)}</p>`;
    case "h":
      return `<h2 style="margin:28px 0 12px;font-family:${SANS};font-size:15px;font-weight:600;letter-spacing:0.09em;text-transform:uppercase;color:${C.muted};">${esc(b.text)}</h2>`;
    case "code":
      // white-space:pre survives Gmail; the horizontal scroll does not, so lines are kept
      // short in the copy rather than relying on the client to wrap them well.
      return `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="margin:0 0 16px;"><tr><td style="background:${C.page};border:1px solid ${C.rule};border-radius:6px;padding:14px 16px;font-family:${MONO};font-size:13px;line-height:1.6;color:${C.ink};white-space:pre;">${esc(b.text)}</td></tr></table>`;
    case "button":
      // A bordered table cell, not a styled <a>. Outlook ignores padding on an anchor and
      // the button collapses to the width of its text.
      return `<table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:4px 0 20px;"><tr><td style="background:${C.accent};border-radius:3px;"><a href="${esc(b.href)}" style="display:inline-block;padding:11px 20px;font-family:${SANS};font-size:15px;font-weight:600;color:#ffffff;text-decoration:none;">${esc(b.label)}</a></td></tr></table>`;
    case "link":
      return `<p style="${pStyle}"><a href="${esc(b.href)}" style="color:${C.accent};text-decoration:underline;">${esc(b.label)}</a></p>`;
    case "rule":
      return `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0"><tr><td style="border-top:1px solid ${C.rule};font-size:0;line-height:0;height:1px;">&nbsp;</td></tr></table><div style="height:24px;line-height:24px;">&nbsp;</div>`;
    case "note":
      return `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="margin:0 0 16px;"><tr><td style="background:${C.accentWash};border-left:3px solid ${C.accent};padding:12px 16px;font-family:${SANS};font-size:14px;line-height:1.55;color:${C.ink};">${inline(b.text)}</td></tr></table>`;
    default:
      return "";
  }
}

function blockText(b) {
  switch (b.kind) {
    case "lead":
    case "p":
    case "note":
      return plain(b.text);
    case "h":
      return String(b.text).toUpperCase();
    case "code":
      return String(b.text).split("\n").map((l) => `    ${l}`).join("\n");
    case "button":
    case "link":
      return `${b.label}: ${b.href}`;
    case "rule":
      return "-".repeat(56);
    default:
      return "";
  }
}

/* ------------------------------------------------------------------- shell */

/**
 * The wordmark.
 *
 * Text, in the mono face, with the accent on `.dev`, which is the same shape the site's
 * header uses. It survives image blocking, it costs no request, and it is the one piece of
 * branding that appears before the reader has decided whether to keep reading.
 */
function letterhead() {
  return `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0">
  <tr><td style="padding:0 0 22px;">
    <span style="font-family:${MONO};font-size:16px;font-weight:600;letter-spacing:-0.01em;color:${C.ink};">kaviri</span><span style="font-family:${MONO};font-size:16px;font-weight:600;color:${C.accent};">.dev</span>
  </td></tr>
</table>`;
}

function footer({ unsubscribeUrl, reason }) {
  const small = `font-family:${SANS};font-size:12px;line-height:1.6;color:${C.faint};`;
  return `<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="margin-top:8px;">
  <tr><td style="border-top:1px solid ${C.rule};padding-top:18px;">
    <p style="margin:0 0 6px;${small}">${esc(reason)}</p>
    <p style="margin:0;${small}">
      <a href="https://kaviri.dev" style="color:${C.faint};text-decoration:underline;">kaviri.dev</a>
      &nbsp;&middot;&nbsp;
      <a href="https://github.com/thisisisheanesu/kaviri" style="color:${C.faint};text-decoration:underline;">source</a>
      ${unsubscribeUrl ? `&nbsp;&middot;&nbsp;<a href="${esc(unsubscribeUrl)}" style="color:${C.faint};text-decoration:underline;">unsubscribe</a>` : ""}
    </p>
  </td></tr>
</table>`;
}

/**
 * Render one email.
 *
 * Returns `{ html, text }`. Both are always produced: a text/plain alternative is not a
 * courtesy, it is the difference between landing in the inbox and landing in spam, and it is
 * what a terminal mail client and a screen reader actually get.
 */
export function render({ preheader, blocks, unsubscribeUrl, reason }) {
  const body = blocks.map(blockHtml).join("\n");

  /*
   * The preheader: the grey line a client shows after the subject. Without one it shows the
   * first words of the body, which here would be the wordmark. Hidden with the usual four
   * declarations, because no single one of them works everywhere, then padded with
   * zero-width joiners so the client does not spill the body text in after it.
   */
  const hidden =
    `<div style="display:none;max-height:0;overflow:hidden;mso-hide:all;font-size:1px;line-height:1px;color:${C.paper};opacity:0;">` +
    `${esc(preheader)}${"&#8204;&nbsp;".repeat(60)}</div>`;

  const html = `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="supported-color-schemes" content="light dark">
<style>
  /* Apple Mail and iOS honour this. Gmail drops the whole block, which is why the inline
     light styles above have to stand on their own. */
  @media (prefers-color-scheme: dark) {
    .kv-page { background: #0b0b0c !important; }
    .kv-card { background: #141416 !important; border-color: #26262a !important; }
    .kv-card p, .kv-card td, .kv-card span, .kv-card h2 { color: #f2f2ef !important; }
    .kv-card a { color: #ff6b3d !important; }
    .kv-card code { background: #1f1f22 !important; color: #f2f2ef !important; }
  }
  @media only screen and (max-width: 620px) {
    .kv-card { padding: 24px 20px !important; }
  }
</style>
</head>
<body class="kv-page" style="margin:0;padding:0;background:${C.page};">
${hidden}
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" class="kv-page" style="background:${C.page};">
  <tr><td align="center" style="padding:32px 16px;">
    <table role="presentation" width="600" cellpadding="0" cellspacing="0" border="0" style="width:600px;max-width:100%;">
      <tr><td class="kv-card" style="background:${C.paper};border:1px solid ${C.rule};border-radius:10px;padding:32px 36px;">
${letterhead()}
${body}
${footer({ unsubscribeUrl, reason })}
      </td></tr>
    </table>
  </td></tr>
</table>
</body></html>`;

  const text = [
    "kaviri.dev",
    "",
    ...blocks.map(blockText).filter((s) => s !== ""),
    "",
    "-".repeat(56),
    reason,
    "https://kaviri.dev",
    unsubscribeUrl ? `Unsubscribe: ${unsubscribeUrl}` : null,
  ]
    .filter((l) => l !== null)
    .join("\n\n")
    .replace(/\n{3,}/g, "\n\n");

  return { html, text };
}
