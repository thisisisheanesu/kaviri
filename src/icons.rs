//! The built-in icons, drawn as small illustrations in the idiom of a
//! desktop app icon: a tile of its own colour, a subject with some depth.
//! All are kaviri's own artwork; none is any vendor's icon.
//!
//! Each is the inside of a 100 unit tile. The caller clips it to a shape
//! (a squircle on macOS, a rounded square elsewhere), supplies `IDsh`, a
//! soft drop shadow for the subject, and replaces `ID` with a prefix unique
//! to the page so gradients never collide.

pub const ART: &[(&str, &str)] = &[
    (
        "files",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fbfcfd"/><stop offset="1" stop-color="#dfe4ea"/></linearGradient><linearGradient id="IDbk" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4aa6ee"/><stop offset="1" stop-color="#2a82d8"/></linearGradient><linearGradient id="IDfr" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#8ed2ff"/><stop offset="1" stop-color="#4aa3f2"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><path d="M15 31a5 5 0 0 1 5-5h19a5 5 0 0 1 4 2l4 5h33a5 5 0 0 1 5 5v35H15z" fill="url(#IDbk)"/><g filter="url(#IDsh)"><path d="M15 42a5 5 0 0 1 5-5h60a5 5 0 0 1 5 5v33a5 5 0 0 1-5 5H20a5 5 0 0 1-5-5z" fill="url(#IDfr)"/></g><path d="M20 38.2h60" stroke="#fff" stroke-opacity=".7" stroke-width="1.2" stroke-linecap="round"/>"##,
    ),
    (
        "browser",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#e6e9ee"/></linearGradient><radialGradient id="IDsp" cx=".36" cy=".3" r=".8"><stop offset="0" stop-color="#9fe6ff"/><stop offset=".45" stop-color="#2f91f2"/><stop offset="1" stop-color="#1147b8"/></radialGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><circle cx="50" cy="50" r="34" fill="url(#IDsp)"/></g><g fill="none" stroke="#fff" stroke-opacity=".55" stroke-width="1.8"><ellipse cx="50" cy="50" rx="14" ry="34"/><path d="M16 50h68M21 34q29-7 58 0M21 66q29 7 58 0M50 16v68"/></g><path d="M30 38c4-6 10-9 15-8-3 5-1 9-6 12-4 2-7 0-9-4zM56 58c6-3 13-1 15 3-2 6-8 9-12 8-3-3-5-7-3-11z" fill="#7ee08f" fill-opacity=".85"/><ellipse cx="41" cy="31" rx="17" ry="9" fill="#fff" fill-opacity=".28"/><circle cx="50" cy="50" r="34" fill="none" stroke="#0b3d99" stroke-opacity=".25"/>"##,
    ),
    (
        "mail",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#46b5ff"/><stop offset="1" stop-color="#0864de"/></linearGradient><linearGradient id="IDen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#e3eaf3"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><rect x="16" y="28" width="68" height="46" rx="6" fill="url(#IDen)"/></g><path d="M17 72 42 50M83 72 58 50" stroke="#c9d4e2" stroke-width="1.8"/><path d="M17 31l33 25 33-25" fill="none" stroke="#b7c5d8" stroke-width="2.4" stroke-linejoin="round"/>"##,
    ),
    (
        "chat",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#6ff58f"/><stop offset="1" stop-color="#16b843"/></linearGradient><linearGradient id="IDbu" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#eefaf1"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><path d="M50 21c20 0 36 13.5 36 30s-16 30-36 30c-5 0-9.5-.8-13.7-2.3L20 84l4.8-13.6C18 65 14 58.4 14 51c0-16.5 16-30 36-30z" fill="url(#IDbu)"/></g>"##,
    ),
    (
        "music",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ff7a8f"/><stop offset="1" stop-color="#f7203f"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)" fill="#fff"><path d="M39 30 77 21v10l-38 9z"/><rect x="39" y="30" width="5.5" height="40"/><rect x="71.5" y="22" width="5.5" height="40"/><ellipse cx="34" cy="70" rx="10" ry="7.5" transform="rotate(-20 34 70)"/><ellipse cx="66.5" cy="62" rx="10" ry="7.5" transform="rotate(-20 66.5 62)"/></g>"##,
    ),
    (
        "photos",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#eeeef1"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g style="mix-blend-mode:multiply"><ellipse cx="50" cy="30" rx="12.5" ry="20" fill="#ffb300" fill-opacity=".82" transform="rotate(0 50 50)" style="mix-blend-mode:multiply"/><ellipse cx="50" cy="30" rx="12.5" ry="20" fill="#ff5b3a" fill-opacity=".82" transform="rotate(60 50 50)" style="mix-blend-mode:multiply"/><ellipse cx="50" cy="30" rx="12.5" ry="20" fill="#ff2f8e" fill-opacity=".82" transform="rotate(120 50 50)" style="mix-blend-mode:multiply"/><ellipse cx="50" cy="30" rx="12.5" ry="20" fill="#9057ff" fill-opacity=".82" transform="rotate(180 50 50)" style="mix-blend-mode:multiply"/><ellipse cx="50" cy="30" rx="12.5" ry="20" fill="#2c9bff" fill-opacity=".82" transform="rotate(240 50 50)" style="mix-blend-mode:multiply"/><ellipse cx="50" cy="30" rx="12.5" ry="20" fill="#35cd6a" fill-opacity=".82" transform="rotate(300 50 50)" style="mix-blend-mode:multiply"/></g>"##,
    ),
    (
        "calendar",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#f0f0f3"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><text x="50" y="31" text-anchor="middle" font-family="-apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Helvetica Neue', Inter, Arial, sans-serif" font-size="15" font-weight="600" fill="#ff3b30" letter-spacing=".5">TUE</text><text x="50" y="80" text-anchor="middle" font-family="-apple-system, BlinkMacSystemFont, 'SF Pro Display', 'Helvetica Neue', Inter, Arial, sans-serif" font-size="54" font-weight="300" fill="#1d1d1f">9</text>"##,
    ),
    (
        "notes",
        r##"<defs><linearGradient id="IDtop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffe27d"/><stop offset="1" stop-color="#ffc930"/></linearGradient></defs><rect width="100" height="100" fill="#fdfdfc"/><rect width="100" height="30" fill="url(#IDtop)"/><path d="M0 30h100" stroke="#d6a300" stroke-width="1.6" stroke-dasharray="1.5 3"/><path d="M14 46h72M14 58h72M14 70h72M14 82h48" stroke="#d8d8dd" stroke-width="1.6" stroke-linecap="round"/>"##,
    ),
    (
        "settings",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#c3c8cf"/><stop offset="1" stop-color="#7b828c"/></linearGradient><linearGradient id="IDg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5d636c"/><stop offset="1" stop-color="#2c3036"/></linearGradient><radialGradient id="IDhub" cx=".5" cy=".4" r=".7"><stop offset="0" stop-color="#eef1f4"/><stop offset="1" stop-color="#a6adb6"/></radialGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><path d="M79.50 50.00 L79.29 53.54 L85.59 55.45 L83.54 63.08 L77.13 61.57 L75.55 64.75 L73.59 67.71 L78.10 72.51 L72.51 78.10 L67.71 73.59 L64.75 75.55 L61.57 77.13 L63.08 83.54 L55.45 85.59 L53.54 79.29 L50.00 79.50 L46.46 79.29 L44.55 85.59 L36.92 83.54 L38.43 77.13 L35.25 75.55 L32.29 73.59 L27.49 78.10 L21.90 72.51 L26.41 67.71 L24.45 64.75 L22.87 61.57 L16.46 63.08 L14.41 55.45 L20.71 53.54 L20.50 50.00 L20.71 46.46 L14.41 44.55 L16.46 36.92 L22.87 38.43 L24.45 35.25 L26.41 32.29 L21.90 27.49 L27.49 21.90 L32.29 26.41 L35.25 24.45 L38.43 22.87 L36.92 16.46 L44.55 14.41 L46.46 20.71 L50.00 20.50 L53.54 20.71 L55.45 14.41 L63.08 16.46 L61.57 22.87 L64.75 24.45 L67.71 26.41 L72.51 21.90 L78.10 27.49 L73.59 32.29 L75.55 35.25 L77.13 38.43 L83.54 36.92 L85.59 44.55 L79.29 46.46Z" fill="url(#IDg)"/><circle cx="50" cy="50" r="22" fill="url(#IDhub)"/><circle cx="50" cy="50" r="22" fill="none" stroke="#2c3036" stroke-width="2"/><circle cx="50" cy="50" r="8" fill="#4a5058"/></g>"##,
    ),
    (
        "terminal",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4a4a4e"/><stop offset="1" stop-color="#1b1b1d"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><rect x="11" y="15" width="78" height="66" rx="7" fill="#0c0c0e" stroke="#5a5a60" stroke-width="1.2"/><path d="M24 38l12 10-12 10" fill="none" stroke="#f2f2f2" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/><path d="M42 60h18" stroke="#f2f2f2" stroke-width="5" stroke-linecap="round"/>"##,
    ),
    (
        "code",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#63b1ff"/><stop offset="1" stop-color="#2250e6"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)" fill="none" stroke="#fff" stroke-linecap="round" stroke-linejoin="round"><path d="M37 31 20 50l17 19M63 31l17 19-17 19" stroke-width="7.5"/><path d="M56 26 44 74" stroke-width="6.5" stroke-opacity=".9"/></g>"##,
    ),
    (
        "camera",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#6a6f77"/><stop offset="1" stop-color="#26282c"/></linearGradient><radialGradient id="IDle" cx=".4" cy=".35" r=".7"><stop offset="0" stop-color="#8a7dff"/><stop offset=".6" stop-color="#2a2170"/><stop offset="1" stop-color="#0b0a1c"/></radialGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><rect x="37" y="23" width="26" height="10" rx="3.5" fill="#1b1c20"/><rect x="13" y="30" width="74" height="48" rx="9" fill="#1b1c20" stroke="#44474e" stroke-width="1.2"/><circle cx="50" cy="54" r="18" fill="#0d0e11" stroke="#7a7e87" stroke-width="3"/><circle cx="50" cy="54" r="11" fill="url(#IDle)"/><circle cx="45.5" cy="49.5" r="3" fill="#fff" fill-opacity=".75"/><circle cx="76" cy="39" r="3" fill="#ffb000"/></g>"##,
    ),
    (
        "video",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#34343a"/><stop offset="1" stop-color="#121214"/></linearGradient><linearGradient id="IDsc" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ff9a4d"/><stop offset="1" stop-color="#ff2e55"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><rect x="14" y="24" width="72" height="52" rx="11" fill="url(#IDsc)"/><path d="M44 39 61 50 44 61z" fill="#fff" stroke="#fff" stroke-width="4" stroke-linejoin="round"/></g>"##,
    ),
    (
        "maps",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#e3f4d4"/><stop offset="1" stop-color="#bfe4a8"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><path d="M62 0h38v46C88 44 74 36 68 24 64 16 62 8 62 0z" fill="#8fd1ff"/><path d="M-4 76C22 66 44 80 104 44" fill="none" stroke="#fff" stroke-width="10"/><path d="M-4 76C22 66 44 80 104 44" fill="none" stroke="#f7c843" stroke-width="5"/><path d="M20 100 42 50" stroke="#fff" stroke-width="6"/><g filter="url(#IDsh)"><path d="M56 14c9 0 16 7 16 16 0 11-16 26-16 26S40 41 40 30c0-9 7-16 16-16z" fill="#ff3b30"/><circle cx="56" cy="30" r="6" fill="#fff"/></g>"##,
    ),
    (
        "store",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#54b8ff"/><stop offset="1" stop-color="#155ff0"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><path d="M38 40v-8a12 12 0 0 1 24 0v8" fill="none" stroke="#fff" stroke-width="5" stroke-linecap="round"/><path d="M25 38h50l-3.5 40a5 5 0 0 1-5 4.5h-33a5 5 0 0 1-5-4.5z" fill="#fff"/><circle cx="38" cy="47" r="2.6" fill="#1f6cf2"/><circle cx="62" cy="47" r="2.6" fill="#1f6cf2"/></g>"##,
    ),
    (
        "ai",
        r##"<defs><radialGradient id="IDbg" cx=".3" cy=".25" r="1"><stop offset="0" stop-color="#d48bff"/><stop offset=".55" stop-color="#6b33ff"/><stop offset="1" stop-color="#2a0f9e"/></radialGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)" fill="#fff"><path d="M46 20c3 20 10 27 30 30-20 3-27 10-30 30-3-20-10-27-30-30 20-3 27-10 30-30z"/><path d="M75 16c1.2 7.5 4 10.3 11.5 11.5C79 28.7 76.2 31.5 75 39c-1.2-7.5-4-10.3-11.5-11.5C71 26.3 73.8 23.5 75 16z" fill-opacity=".85"/></g>"##,
    ),
    (
        "game",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ff72c8"/><stop offset="1" stop-color="#6a38ff"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><path d="M28 34h44c9 0 15 9 16 22 1 11-3 18-9 18-5 0-8-5-12-10H33c-4 5-7 10-12 10-6 0-10-7-9-18 1-13 7-22 16-22z" fill="#fff"/><path d="M28 45v14M21 52h14" stroke="#6a38ff" stroke-width="4.5" stroke-linecap="round"/><circle cx="68" cy="47" r="3.6" fill="#ff4fa3"/><circle cx="76" cy="55" r="3.6" fill="#5a8cff"/></g>"##,
    ),
    (
        "wallet",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2e2e32"/><stop offset="1" stop-color="#0c0c0e"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><rect x="16" y="20" width="68" height="22" rx="4" fill="#ffb000"/><rect x="16" y="30" width="68" height="22" rx="4" fill="#ff453a"/><rect x="16" y="40" width="68" height="22" rx="4" fill="#32d15f"/><rect x="16" y="50" width="68" height="22" rx="4" fill="#0a84ff"/><g filter="url(#IDsh)"><path d="M12 58q38 14 76 0v22a6 6 0 0 1-6 6H18a6 6 0 0 1-6-6z" fill="#1c1c1f"/></g>"##,
    ),
    (
        "device",
        r##"<defs><linearGradient id="IDbg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#eef0f3"/><stop offset="1" stop-color="#c3c8cf"/></linearGradient><linearGradient id="IDsc" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5ac8fa"/><stop offset="1" stop-color="#5856d6"/></linearGradient></defs><rect width="100" height="100" fill="url(#IDbg)"/><g filter="url(#IDsh)"><rect x="31" y="11" width="38" height="78" rx="9" fill="#1d1d1f"/><rect x="34.5" y="15" width="31" height="70" rx="6" fill="url(#IDsc)"/><rect x="44" y="18.5" width="12" height="3.8" rx="1.9" fill="#000"/></g>"##,
    ),
];

pub fn art(name: &str) -> Option<&'static str> {
    ART.iter().find(|a| a.0 == name).map(|a| a.1)
}
