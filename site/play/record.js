/*
 * Giving the playground something to download.
 *
 * There is no ffmpeg in a browser tab, so this is not the renderer and does not pretend to be.
 * It asks the browser to capture this tab while the script runs, and hands back the WebM the
 * browser produces. What you get is genuinely the camera kaviri will use, at whatever your
 * screen happens to be, with none of the things the binary does after the fact: no 2x
 * supersampled capture, no backdrop, no drawn cursor, no H.264.
 *
 * The browser shows its own picker and its own recording indicator throughout. Nothing here
 * can start a capture the person did not choose in an interface that belongs to the browser.
 */

export function canRecord() {
  return Boolean(navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia && window.MediaRecorder);
}

/** The best container the browser will actually give us, most preferred first. */
function pickMime() {
  const wanted = [
    "video/webm;codecs=vp9",
    "video/webm;codecs=vp8",
    "video/webm",
    "video/mp4",
  ];
  return wanted.find((m) => MediaRecorder.isTypeSupported(m)) || "";
}

export async function startCapture() {
  const stream = await navigator.mediaDevices.getDisplayMedia({
    // A hint, not a guarantee: Chrome puts this tab at the top of its own picker, and the
    // person still has to choose it. Firefox ignores the hint and shows the full picker.
    preferCurrentTab: true,
    video: { frameRate: 30 },
    audio: false,
  });

  const mimeType = pickMime();
  const rec = new MediaRecorder(stream, mimeType ? { mimeType, videoBitsPerSecond: 6_000_000 } : undefined);
  const chunks = [];
  rec.ondataavailable = (e) => {
    if (e.data && e.data.size) chunks.push(e.data);
  };

  let stoppedByUser = false;
  // The browser's own "stop sharing" button ends the track without ending the recorder, so a
  // take stopped that way would otherwise hang waiting for a stop that never comes.
  stream.getVideoTracks().forEach((t) => {
    t.addEventListener("ended", () => {
      stoppedByUser = true;
      if (rec.state !== "inactive") rec.stop();
    });
  });

  rec.start(250);

  return {
    mimeType: mimeType || "video/webm",
    get abandoned() {
      return stoppedByUser;
    },
    async stop() {
      if (rec.state === "inactive") {
        stream.getTracks().forEach((t) => t.stop());
        return chunks.length ? new Blob(chunks, { type: mimeType || "video/webm" }) : null;
      }
      const done = new Promise((resolve) => rec.addEventListener("stop", resolve, { once: true }));
      rec.stop();
      await done;
      stream.getTracks().forEach((t) => t.stop());
      return chunks.length ? new Blob(chunks, { type: mimeType || "video/webm" }) : null;
    },
  };
}

export function offer(blob, name) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  // Long enough for the download to have been handed to the browser, then the memory goes.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/** The script itself is worth downloading too, and it is the thing the binary actually takes. */
export function offerScript(text) {
  offer(new Blob([text], { type: "application/jsonl" }), "take.jsonl");
}
