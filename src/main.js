/* ============================================================
   AdSlicer — Frontend Logic
   ============================================================ */

const T      = window.__TAURI__ || {};
const core   = T.core;
const dialog = T.dialog;
const event  = T.event;

function openModal(url, title) {
  const modal = document.getElementById("web-modal");
  const frame = document.getElementById("web-modal-frame");
  const label = document.getElementById("web-modal-title");
  label.textContent = title || "AdSlicer Docs";
  frame.src = url;
  modal.style.display = "flex";
}

function closeModal() {
  const modal = document.getElementById("web-modal");
  const frame = document.getElementById("web-modal-frame");
  modal.style.display = "none";
  frame.src = "";   // stop any loading
}

function $(id) { return document.getElementById(id); }

let jobRunning = false;
// ─── Crash-proofing: surface frontend errors instead of letting the WebView die ───
window.addEventListener("error", (e) => {
  try {
    const msg = e?.message || "Unknown error";
    const src = e?.filename ? ` (${e.filename}:${e.lineno || 0}:${e.colno || 0})` : "";
    if (document.getElementById("console")) {
      logLine("error", `[${timestamp()}]  ✖  Frontend error: ${msg}${src}`);
    } else {
      console.error("Frontend error:", msg, src);
    }
  } catch (_) {}
});

window.addEventListener("unhandledrejection", (e) => {
  try {
    const reason = e?.reason;
    const msg = (reason && (reason.message || String(reason))) || "Unhandled promise rejection";
    if (document.getElementById("console")) {
      logLine("error", `[${timestamp()}]  ✖  Unhandled rejection: ${msg}`);
    } else {
      console.error("Unhandled rejection:", msg);
    }
  } catch (_) {}
});

/* ─── Mode helpers ─── */
function mode() {
  const m = document.querySelector('input[name="mode"]:checked');
  return m ? m.value : "singleFile";
}

function updateInputLabel() {
  $("inputLabel").textContent = mode() === "batchDir" ? "Input Folder" : "Input Path";
  const val = $("inputPath").value;
  $("sb-file").textContent = val ? truncatePath(val) : "No file selected";
}

function truncatePath(p) {
  if (p.length <= 40) return p;
  return "…" + p.slice(-38);
}

/* ─── File pickers ─── */
async function pickInput() {
  if (!dialog) { logLine("system", "[SYS] Dialog API unavailable"); return; }
  const dir = mode() === "batchDir";
  const sel = await dialog.open({ directory: dir, multiple: false });
  if (sel) {
    $("inputPath").value = sel;
    $("sb-file").textContent = truncatePath(sel);
  }
}

async function pickOutput() {
  if (!dialog) { logLine("system", "[SYS] Dialog API unavailable"); return; }
  const sel = await dialog.open({ directory: true, multiple: false });
  if (sel) { $("outputDir").value = sel; }
}

/* ─── Collect params ─── */
function collectParams() {
  return {
    inputMode:     mode(),
    inputPath:     $("inputPath").value.trim(),
    glob:          $("globPattern").value.trim() || "*.mp4,*.mov,*.mkv,*.avi,*.m4v,*.wmv,*.flv,*.webm,*.mpg,*.mpeg,*.mts,*.m2ts,*.ts,*.vob,*.3gp,*.dv",
    outdir:        $("outputDir").value.trim(),
    mediaType:     "mp4",
    blackMinDur:   parseFloat($("blackMinDur").value),
    pixTh:         parseFloat($("pixTh").value),
    picTh:         parseFloat($("picTh").value),
    mergeGap:      parseFloat($("mergeGap").value),
    edgePadPre:    parseFloat($("edgePadPre").value),
    edgePadPost:   parseFloat($("edgePadPost").value),
    minCommercial: parseFloat($("minCommercial").value),
    maxCommercial: parseFloat($("maxCommercial").value),
    includeBlack:    $("includeBlack").checked,
    dryRun:          $("dryRun").checked,
    previewDur:      parseFloat($("previewDur").value) || 0,
    outputMode:      $("outputMode").value,
    encode: {
      mode:              $("encodeMode").value,
      gpuAccel:          $("gpuAccel").value,
      videoCrf:          parseInt($("videoCrf").value, 10),
      videoPreset:       $("videoPreset").value,
      audioCodec:        $("audioCodec").value,
      audioBitrateKbps:  parseInt($("audioBitrateKbps").value, 10),
      loudnorm:          $("loudnorm").checked,
      deinterlace:       $("deinterlace").checked,
      scaleWidth:        parseInt($("scaleWidth").value, 10),
    },
    reencode:        false,  // legacy field — kept for preset compat
    verbosity:       parseInt($("verbosity").value, 10),
    // ── Comskip-derived enhancements ─────────────────────────────────
    silenceNoiseDb:  parseFloat($("silenceNoiseDb").value),
    silenceMinDur:   parseFloat($("silenceMinDur").value),
    minShowSegment:  parseFloat($("minShowSegment").value),
    alwaysKeepFirst: parseFloat($("alwaysKeepFirst").value),
    alwaysKeepLast:  parseFloat($("alwaysKeepLast").value),
    uniformMaxStddev: parseFloat($("uniformMaxStddev").value),
    sceneThreshold:  parseFloat($("sceneThreshold").value),
    removeBefore:    parseFloat($("removeBefore").value),
    removeAfter:     parseFloat($("removeAfter").value),
    requireDiv5:     $("requireDiv5").checked,
    trimHead:        parseFloat($("trimHead").value) || 0,
    trimTail:        parseFloat($("trimTail").value) || 0,
    postCommand:     $("postCommand").value.trim(),
  };
}

/* ─── Logger ─── */

// ─── Console output (batched + capped to prevent WebView crashes) ──────────────
const MAX_LOG_LINES = 6000;
let _logQueue = [];
let _logFlushScheduled = false;
let _logLineCount = 0;

// We keep one text buffer instead of creating thousands of DOM nodes.
function _consoleEl() { return $("console"); }

function _scheduleFlush() {
  if (_logFlushScheduled) return;
  _logFlushScheduled = true;
  requestAnimationFrame(() => {
    _logFlushScheduled = false;
    if (_logQueue.length === 0) return;

    const el = _consoleEl();
    // Ensure pre-like formatting even if console is a div
    el.style.whiteSpace = "pre-wrap";

    // Append to a single textContent buffer
    let chunk = _logQueue.join("\n") + "\n";
    _logQueue = [];

    // Track approximate line count, and cap by trimming occasionally
    _logLineCount += (chunk.match(/\n/g) || []).length;

    // Append
    el.textContent += chunk;

    // Cap: if too many lines, trim from the front (expensive, but only when exceeding cap)
    if (_logLineCount > MAX_LOG_LINES) {
      const lines = el.textContent.split(/\r?\n/);
      const kept = lines.slice(-MAX_LOG_LINES);
      el.textContent = kept.join("\n");
      _logLineCount = kept.length;
    }

    el.scrollTop = el.scrollHeight;
  });
}

function logLine(type, text) {
  // Keep type in the text (UI styling is handled elsewhere); this avoids DOM spam.
  _logQueue.push(text);
  _scheduleFlush();
}

function logBlank() {
  _logQueue.push("");
  _scheduleFlush();
}

function logSeparator() {
  logLine("system", "──────────────────────────────────────────────");
}

function timestamp() {
  return new Date().toTimeString().slice(0, 8);
}

/* ─── Log parser ─── */
function parseAndLog(raw) {
  if (!raw || !raw.trim()) return;
  for (const rawLine of raw.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line) classifyAndLog(line);
  }
}

function classifyAndLog(line) {
  const ts = `[${timestamp()}]`;

  // ── ffmpeg noise filter (must run BEFORE ffmpeg version check below) ────────
  if (isFFmpegNoise(line)) return;

  // ── Job terminal states ─────────────────────────────────────────────────────
  if (/\[done\]/i.test(line) || /job complete/i.test(line)) {
    logBlank();
    logLine("success", `${ts}  ✔  JOB COMPLETE — All files processed successfully.`);
    logSeparator();
    return;
  }

  if (/\[error\]|\berror\b|exception|traceback|failed/i.test(line)) {
    const clean = line.replace(/^\[error\]\s*/i, "").replace(/^[EW]\s+\S+\s*\|\s*/, "");
    logLine("error", `${ts}  ✖  ${clean}`);
    return;
  }

  // ── Binary / environment ────────────────────────────────────────────────────
  if (/Using ffmpeg:|Using ffprobe:/i.test(line)) {
    logLine("system", `${ts}  ◆  ${line.trim()}`);
    return;
  }

  // ffmpeg version string emitted once per job (NOT caught by isFFmpegNoise
  // because that filter only fires on lines starting with "ffmpeg version").
  // Our backend emits "ffmpeg: ffmpeg version 6.x …" with a prefix.
  const ffverMatch = line.match(/^ffmpeg:\s*(.+)/i);
  if (ffverMatch) {
    logLine("system", `${ts}  ◆  ${ffverMatch[1].trim()}`);
    return;
  }

  // ── File header ─────────────────────────────────────────────────────────────
  const analyzeMatch = line.match(/Analyzing:\s*(.+)/i);
  if (analyzeMatch) {
    logBlank();
    logLine("header", `${ts}  ►  Analyzing: ${analyzeMatch[1]}`);
    return;
  }

  const durMatch = line.match(/ffprobe duration:\s*([0-9.]+)s/i);
  if (durMatch) {
    logLine("info", `${ts}  ◎  Source duration: ${formatDuration(parseFloat(durMatch[1]))}`);
    return;
  }

  // ── Black-frame detection ───────────────────────────────────────────────────
  const rawBlackMatch = line.match(/Detected\s+(\d+)\s+raw black/i);
  if (rawBlackMatch) {
    logLine("info", `${ts}  ●  Raw black segments detected: ${rawBlackMatch[1]}`);
    return;
  }

  const filteredMatch = line.match(/After filtering.*?:\s*(\d+)\s+black/i);
  if (filteredMatch) {
    logLine("info", `${ts}  ●  Black segments after filter/merge: ${filteredMatch[1]}`);
    return;
  }

  // ── Silence detection ───────────────────────────────────────────────────────
  const uniformRunMatch = line.match(/Running uniform detection\s*\(max_stddev=([^)]+)\)/i);
  if (uniformRunMatch) {
    logLine("info", `${ts}  ▦  Uniform scan  max_stddev=${uniformRunMatch[1].trim()}`);
    return;
  }

  const uniformFoundMatch = line.match(/Detected\s+(\d+)\s+uniform/i);
  if (uniformFoundMatch) {
    const n = parseInt(uniformFoundMatch[1]);
    logLine("info", `${ts}  ▦  Uniform segments: ${n === 0 ? "none" : n}`);
    return;
  }

  const sceneRunMatch = line.match(/Running scene change detection\s*\(threshold=([^)]+)\)/i);
  if (sceneRunMatch) {
    logLine("info", `${ts}  ⋯  Scene scan  threshold=${sceneRunMatch[1].trim()}`);
    return;
  }

  const sceneFoundMatch = line.match(/Detected\s+(\d+)\s+scene change.*?([0-9.]+)\/s/i);
  if (sceneFoundMatch) {
    logLine("info", `${ts}  ⋯  Scene changes: ${sceneFoundMatch[1]}  avg ${sceneFoundMatch[2]}/s`);
    return;
  }

    const silenceRunMatch = line.match(/Running silence detection\s*\(noise_db=([^,]+),\s*min_dur=([^)]+)\)/i);
  if (silenceRunMatch) {
    logLine("info", `${ts}  ♪  Silence scan  noise=${silenceRunMatch[1].trim()}dB  min=${silenceRunMatch[2].trim()}`);
    return;
  }

  const silenceFoundMatch = line.match(/Detected\s+(\d+)\s+silence/i);
  if (silenceFoundMatch) {
    const n = parseInt(silenceFoundMatch[1]);
    if (n === 0) {
      logLine("info", `${ts}  ♪  No silence segments found`);
    } else {
      logLine("info", `${ts}  ♪  Silence segments: ${n}`);
    }
    return;
  }

  if (/Silence detection disabled/i.test(line)) {
    logLine("system", `${ts}  ♪  Silence detection off`);
    return;
  }

  if (/Silence detection failed/i.test(line)) {
    logLine("warn", `${ts}  ⚠  Silence detection failed (continuing without it)`);
    return;
  }

  // ── Plan summary ────────────────────────────────────────────────────────────
  const planMatch = line.match(/Plan:\s*(\d+)\s+commercial.*?(\d+)\s+keep/i);
  if (planMatch) {
    logBlank();
    logLine("info", `${ts}  ├─ Commercial blocks : ${planMatch[1]}`);
    logLine("info", `${ts}  └─ Show segments     : ${planMatch[2]}`);
    logBlank();
    return;
  }

  if (/No commercials detected/i.test(line)) {
    logLine("warn", `${ts}  ⚠  No commercial breaks detected.`);
    return;
  }

  // ── Commercial listing (now includes conf= and signals) ─────────────────────
  // Format: "  001 | 00:04:12.000 -> 00:08:45.000 | 00:04:33.000 | conf=1.05 | [black_boundary, silence_overlap]"
  const commListMatch = line.match(/^\s+(\d+)\s*\|\s*([\d:.]+)\s*->\s*([\d:.]+)\s*\|\s*([\d:.]+)\s*\|\s*conf=([0-9.]+)\s*\|\s*\[([^\]]*)\]/i);
  if (commListMatch) {
    const idx    = commListMatch[1].trim().padStart(2, "0");
    const start  = commListMatch[2].trim();
    const end    = commListMatch[3].trim();
    const dur    = commListMatch[4].trim();
    const conf   = parseFloat(commListMatch[5]);
    const sigs   = commListMatch[6].trim();
    const confBar = conf >= 1.0 ? "★★★" : conf >= 0.8 ? "★★☆" : "★☆☆";
    const sigShort = sigs ? `  [${sigs}]` : "";
    logLine("info", `${ts}  ✂  AD #${idx}  ${start} → ${end}  (${dur})  ${confBar}${sigShort}`);
    return;
  }

  // ── Cut operations ──────────────────────────────────────────────────────────
  // Format includes conf= and signals: "Cut COMM 01: … conf=1.05 [sig, sig] -> path"
  const commCutMatch = line.match(/Cut COMM\s+(\d+):\s*([\d:.]+)\s*[→\->]+\s*([\d:.]+)\s*\(([^)]+)\)\s*conf=([0-9.]+)/i);
  if (commCutMatch) {
    const conf = parseFloat(commCutMatch[5]);
    const confBar = conf >= 1.0 ? "★★★" : conf >= 0.8 ? "★★☆" : "★☆☆";
    logLine("debug", `${ts}  ✂  CUT AD #${commCutMatch[1].padStart(2,"0")}  ${commCutMatch[2]} → ${commCutMatch[3]}  (${commCutMatch[4]})  ${confBar}`);
    return;
  }

  const keepCutMatch = line.match(/Cut KEEP\s+(\d+):\s*([\d:.]+)\s*[→\->]+\s*([\d:.]+)\s*\(([^)]+)\)/i);
  if (keepCutMatch) {
    logLine("debug", `${ts}  ░  KEEP #${keepCutMatch[1].padStart(2,"0")}  ${keepCutMatch[2]} → ${keepCutMatch[3]}  (${keepCutMatch[4]})`);
    return;
  }

  // ── Comskip feature summary block ──────────────────────────────────────────
  if (/^Detection summary:/i.test(line)) {
    logBlank();
    logLine("header", `${ts}  ◈  Detection summary`);
    return;
  }

  const silScanMatch = line.match(/Silence scan\s*:\s*(\d+) segment.*?(\d+) cut.*?boosted/i);
  if (silScanMatch) {
    const found = parseInt(silScanMatch[1]); const boosted = parseInt(silScanMatch[2]);
    const icon = found === 0 ? "○" : "♪";
    logLine("info", `${ts}  ${icon}  Silence    ${found} segment(s)  →  ${boosted} boosted`);
    return;
  }

  const unifScanMatch = line.match(/Uniform scan\s*:\s*(\d+) segment.*?(\d+) cut.*?boosted/i);
  if (unifScanMatch) {
    const uf = parseInt(unifScanMatch[1]); const ub = parseInt(unifScanMatch[2]);
    logLine("info", `${ts}  ${uf===0?"○":"▦"}  Uniform    ${uf} segment(s)  →  ${ub} boosted`);
    return;
  }

  const scScanMatch = line.match(/Scene change\s*:\s*(\d+) change.*?([0-9.]+)\/s.*?(\d+) cut.*?boosted/i);
  if (scScanMatch) {
    const sc = parseInt(scScanMatch[1]); const sr = scScanMatch[2]; const sb = parseInt(scScanMatch[3]);
    logLine("info", `${ts}  ${sc===0?"○":"⋯"}  Scene      ${sc} change(s)  ${sr}/s  →  ${sb} boosted`);
    return;
  }

  const div5Match = line.match(/Div5 snapping\s*:\s*(\d+) cut.*?snapped/i);
  if (div5Match) {
    logLine("info", `${ts}  ○  30s snap    ${div5Match[1]} cut(s) snapped to boundary`);
    return;
  }

  const trimMatch = line.match(/Asymmetric trim\s*:.*remove_before=([0-9.]+).*remove_after=([0-9.]+)/i);
  if (trimMatch) {
    logLine("info", `${ts}  ○  Edge trim   before=${trimMatch[1]}s  after=${trimMatch[2]}s`);
    return;
  }

  const showGuardMatch = line.match(/Show guard\s*:\s*(\d+) candidate.*?demoted.*?min_show_segment = ([0-9.]+)s/i);
  if (showGuardMatch) {
    const demoted = parseInt(showGuardMatch[1]);
    const minSeg  = showGuardMatch[2];
    const icon    = demoted > 0 ? "⊘" : "○";
    logLine("info", `${ts}  ${icon}  Show guard  ${demoted} cut(s) demoted  (min segment ${minSeg}s)`);
    return;
  }

  const edgeProtMatch = line.match(/Edge protection\s*:\s*head = (.+?)\s{2,}tail = (.+)/i);
  if (edgeProtMatch) {
    logLine("info", `${ts}  ○  Edge guard  head: ${edgeProtMatch[1].trim()}  tail: ${edgeProtMatch[2].trim()}`);
    return;
  }

  const commLoadMatch = line.match(/Commercial load\s*:\s*([0-9.]+)s of ([0-9.]+)s total\s*\(([0-9.]+)%\)/i);
  if (commLoadMatch) {
    const pct = parseFloat(commLoadMatch[3]);
    const bar = pct > 40 ? "▰▰▰▰▰" : pct > 25 ? "▰▰▰▱▱" : pct > 12 ? "▰▰▱▱▱" : "▰▱▱▱▱";
    logLine("info", `${ts}  ${bar}  Ad load: ${commLoadMatch[1]}s / ${commLoadMatch[2]}s  (${commLoadMatch[3]}%)`);
    return;
  }

  const keepMatch2 = line.match(/Keep content\s*:\s*([0-9.]+)s across (\d+) segment/i);
  if (keepMatch2) {
    logLine("info", `${ts}  ○  Show content: ${keepMatch2[1]}s across ${keepMatch2[2]} segment(s)`);
    logBlank();
    return;
  }

  // ── Dataset / log output ────────────────────────────────────────────────────
  const datasetMatch = line.match(/Dataset written\s*[→\->]+\s*(.+)/i);
  if (datasetMatch) {
    logLine("system", `${ts}  ⬡  Dataset → ${datasetMatch[1].trim()}`);
    return;
  }

  // ── Dry run ─────────────────────────────────────────────────────────────────
  if (/dry.run/i.test(line)) {
    logLine("warn", `${ts}  ⊘  DRY RUN — No media files created.`);
    return;
  }

  // ── Export results ──────────────────────────────────────────────────────────
  const commFilesMatch = line.match(/Commercial files:\s*(\d+)\s*[→\->]+\s*(.+)/i);
  if (commFilesMatch) {
    logLine("success", `${ts}  ✔  ${commFilesMatch[1]} ad clip(s) saved → ${commFilesMatch[2]}`);
    return;
  }

  const showFileMatch = line.match(/Show file:\s*(.+)/i);
  if (showFileMatch) {
    logLine("success", `${ts}  ✔  Clean show → ${showFileMatch[1]}`);
    return;
  }

  const encodeSummaryMatch = line.match(/^Encode:\s*mode=(\S+).*?gpu=(\S+)/i);
  if (encodeSummaryMatch) {
    const mode = encodeSummaryMatch[1];
    const gpu  = encodeSummaryMatch[2];
    const icon = mode === "copy" ? "○" : "⚙";
    logLine("system", `${ts}  ${icon}  Encode: ${line.replace(/^Encode:\s*/i, "").trim()}`);
    return;
  }

  const previewModeMatch = line.match(/Preview mode:\s*segments capped at (\d+)s/i);
  if (previewModeMatch) {
    logLine("warn", `${ts}  ⧖  Preview mode — segments capped at ${previewModeMatch[1]}s`);
    return;
  }

  const previewLabelMatch = line.match(/\[preview ≤(\d+)s\]/i);
  if (previewLabelMatch) {
    // preview labels are embedded in cut lines — let the normal cut handler show them
  }

  const recordingTrimMatch = line.match(/Recording trim\s*:\s*head=([0-9.]+)s\s*tail=([0-9.]+)s/i);
  if (recordingTrimMatch) {
    const h = parseFloat(recordingTrimMatch[1]); const t = parseFloat(recordingTrimMatch[2]);
    const parts = [];
    if (h > 0) parts.push(`head −${h}s`);
    if (t > 0) parts.push(`tail −${t}s`);
    logLine("info", `${ts}  ✂  Recording trim: ${parts.join('  ')}`);
    return;
  }

  const postCmdMatch = line.match(/^Post-processing:\s*(.+)/i);
  if (postCmdMatch) {
    logLine("system", `${ts}  ◆  Post: ${postCmdMatch[1].trim()}`);
    return;
  }

  const postDoneMatch = line.match(/Post-processing: completed/i);
  if (postDoneMatch) {
    logLine("success", `${ts}  ✔  Post-processing complete`);
    return;
  }

  const postWarnMatch = line.match(/\[warn\] Post-processing/i);
  if (postWarnMatch) {
    logLine("warn", `${ts}  ⚠  ${line.replace(/^\[warn\]\s*/i,'').trim()}`);
    return;
  }

  const loudnormMatch = line.match(/Applying loudnorm/i);
  if (loudnormMatch) {
    logLine("info", `${ts}  ♫  Loudnorm: applying EBU R128 normalisation…`);
    return;
  }

  const chapteredMatch = line.match(/Chaptered file\s*[→\->]+\s*(.+)/i);
  if (chapteredMatch) {
    logLine("success", `${ts}  ✔  Chaptered file → ${chapteredMatch[1].trim()}`);
    return;
  }

  const embedMatch = line.match(/Embedding chapters into source/i);
  if (embedMatch) {
    logLine("info", `${ts}  ⬡  Embedding chapter markers into source copy…`);
    return;
  }

  const logsMatch = line.match(/^Logs:\s*(.+)/i);
  if (logsMatch) {
    logLine("system", `${ts}  ◆  Logs → ${logsMatch[1].trim()}`);
    return;
  }

  // ── DONE summary block ──────────────────────────────────────────────────────
  // "DONE Done: <file>" triggers a visual separator in the log
  if (/^DONE Done:/i.test(line)) {
    logSeparator();
    return;
  }

  // ── Concat / single-part copy ───────────────────────────────────────────────
  const concatMatch = line.match(/Concatenating\s+(\d+)\s+parts/i);
  if (concatMatch) {
    logLine("info", `${ts}  ⊕  Joining ${concatMatch[1]} segments…`);
    return;
  }

  if (/Single keep segment/i.test(line)) {
    logLine("info", `${ts}  ⊕  Single segment — direct copy`);
    return;
  }

  // ── Generic passthrough ─────────────────────────────────────────────────────
  const cleaned = line
    .replace(/^[IDWE]\s+[\w.]+\s*\|\s*/, "")
    .replace(/^\[python\s+\w+\]\s*/i, "")
    .trim();

  if (!cleaned) return;

  if (/warn|warning/i.test(cleaned)) {
    logLine("warn",  `${ts}  ⚠  ${cleaned}`);
  } else if (/debug|KEEP|COMM/i.test(cleaned)) {
    logLine("debug", `${ts}  ·  ${cleaned}`);
  } else {
    logLine("info",  `${ts}  ○  ${cleaned}`);
  }
}

function isFFmpegNoise(line) {
  if (!line) return true;
  return [
    /^ffmpeg version/i, /^ffprobe version/i, /built with/i,
    /configuration:/i, /libav/i, /encoder\s*:/i,
    /^\s*Stream #/i, /^\s*Input #/i, /^\s*Output #/i,
    /^\s*Metadata:/i, /^\s*Duration:/i,
    /frame=\s*\d/i, /fps=\s*[\d.]+/i, /bitrate=/i,
    /size=\s*\d+/i, /time=[\d:.]+/i, /speed=[\d.]+x/i,
    /Press \[q\]/i, /Qavg:/i, /Lsize:/i, /mux overhead/i,
    /^\s*$/,
  ].some(p => p.test(line));
}

function formatDuration(secs) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return [h, m, s].map(v => String(v).padStart(2, "0")).join(":");
}

/* ─── Status ─── */
function setStatus(state, text) {
  $("status").textContent     = text;
  $("sb-status").textContent  = text;
  $("statusLed").className    = "status-led " + state;
  if (state === "running") {
    $("consoleLed").classList.add("active");
  } else {
    $("consoleLed").classList.remove("active");
  }
}

/* ─── Job control ─── */
async function startJob() {
  if (jobRunning) { logLine("warn", `[${timestamp()}]  ⚠  A job is already running.`); return; }
  if (!core)      { logLine("error", `[${timestamp()}]  ✖  Tauri core API unavailable.`); return; }

  const p = collectParams();
  if (!p.inputPath) { logLine("error", `[${timestamp()}]  ✖  No input path specified.`); return; }
  if (!p.outdir)    { logLine("error", `[${timestamp()}]  ✖  No output directory specified.`); return; }

  jobRunning = true;
  setStatus("running", "Running…");

  logBlank();
  logLine("header", `[${timestamp()}]  ►  JOB START`);
  logLine("info",   `[${timestamp()}]  ●  Input  : ${p.inputPath}`);
  logLine("info",   `[${timestamp()}]  ●  Output : ${p.outdir}`);
  logSeparator();

  try {
    await core.invoke("run_adslicer_job", { params: p });
  } catch (e) {
    logLine("error", `[${timestamp()}]  ✖  ${e}`);
    setStatus("error", "Error");
    jobRunning = false;
  }
}

async function stopJob() {
  if (!core) return;
  try {
    await core.invoke("cancel_adslicer_job");
    logBlank();
    logLine("warn", `[${timestamp()}]  ⊘  Job cancelled by user.`);
    logSeparator();
    setStatus("idle", "Cancelled");
    jobRunning = false;
  } catch (e) {
    logLine("error", `[${timestamp()}]  ✖  Cancel error: ${e}`);
  }
}

/* ═══════════════════════════════════════════════════════════
   TOOLTIP — viewport-clamped, JS-rendered
   The tooltip div lives outside .app-root so the overflow:auto
   scroll container on .column-left cannot clip it.
   ═══════════════════════════════════════════════════════════ */
function initTooltips() {
  const tip = document.getElementById("app-tooltip");
  if (!tip) return;

  const MARGIN = 8;
  const OFFSET = 6;
  let activeEl  = null;
  let showTimer = null;

  function show(el) {
    const text = el.getAttribute("data-tip");
    if (!text) return;
    tip.textContent   = text;
    tip.style.display = "block";

    const eRect = el.getBoundingClientRect();
    const tRect = tip.getBoundingClientRect();
    const vw    = window.innerWidth;

    // Prefer above; fall back below if no room
    let top  = eRect.top - tRect.height - OFFSET;
    if (top < MARGIN) top = eRect.bottom + OFFSET;

    // Align left edge of tooltip to left edge of element; clamp to viewport
    let left = eRect.left;
    if (left + tRect.width > vw - MARGIN) left = vw - tRect.width - MARGIN;
    if (left < MARGIN) left = MARGIN;

    tip.style.top  = top  + "px";
    tip.style.left = left + "px";
  }

  function hide() {
    clearTimeout(showTimer);
    tip.style.display = "none";
    activeEl = null;
  }

  document.addEventListener("mouseover", (e) => {
    const el = e.target.closest("[data-tip]");
    if (!el || el === activeEl) return;
    // Don't show tooltips over menu items — menus take priority
    if (el.closest(".menu-bar")) return;
    activeEl = el;
    clearTimeout(showTimer);
    showTimer = setTimeout(() => show(el), 350);
  });

  document.addEventListener("mouseout", (e) => {
    const el = e.target.closest("[data-tip]");
    if (!el) return;
    if (!el.contains(e.relatedTarget)) {
      clearTimeout(showTimer);
      if (!e.relatedTarget || !el.contains(e.relatedTarget)) hide();
    }
  });

  document.addEventListener("scroll", hide, true);
}

/* ═══════════════════════════════════════════════════════════
   MENU BAR — Win98-style dropdowns
   
   THE CRITICAL BUG THAT KEPT BREAKING THIS:
   A document-level "click outside to close" handler fires on
   the SAME click that opens the menu, because the click bubbles
   up to document after opening. Fix: use a boolean flag set
   during open, checked in the document handler, cleared on
   next tick with setTimeout(..., 0).
   ═══════════════════════════════════════════════════════════ */
function initMenuBar() {
  const menuBar   = document.getElementById("menuBar");
  if (!menuBar) return;

  const items     = Array.from(menuBar.querySelectorAll(".menu-item"));
  let openItem    = null;
  let justOpened  = false;   // guard flag — prevents immediate self-close

  function open(item) {
    if (openItem && openItem !== item) close();
    item.classList.add("open");
    openItem   = item;
    justOpened = true;
    setTimeout(() => { justOpened = false; }, 0);
  }

  function close() {
    if (openItem) {
      openItem.classList.remove("open");
      openItem = null;
    }
  }

  // Click a top-level label → toggle
  items.forEach(item => {
    const label = item.querySelector(".menu-item-label");
    if (!label) return;

    label.addEventListener("click", (e) => {
      e.stopPropagation();
      if (item.classList.contains("open")) {
        close();
      } else {
        open(item);
      }
    });

    // Hover-switch while a menu is already open
    item.addEventListener("mouseenter", () => {
      if (openItem && openItem !== item) open(item);
    });
  });

  // Click a dropdown item → dispatch action then close
  menuBar.addEventListener("click", (e) => {
    const dd = e.target.closest(".menu-dd-item");
    if (!dd) return;
    e.stopPropagation();
    if (dd.classList.contains("disabled")) return;
    const action = dd.getAttribute("data-action");
    close();
    handleMenuAction(action);
  });

  // Click outside the menu bar → close (but not if we JUST opened)
  document.addEventListener("click", () => {
    if (justOpened) return;
    close();
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
}

/* ─── Menu action dispatcher ─── */
function handleMenuAction(action) {
  const ts = timestamp();
  switch (action) {
    case "file-open":         pickInput(); break;
    case "file-open-folder":
      document.querySelector('input[name="mode"][value="batchDir"]').checked = true;
      updateInputLabel();
      pickInput();
      break;
    case "file-output":       pickOutput(); break;
    case "file-save-preset":  openSavePresetModal(); break;
    case "file-load-preset":  pickAndLoadPreset(); break;
    case "edit-reset":
      resetParameters();
      logLine("system", `[${ts}]  ◆  Parameters reset to defaults.`);
      break;
    case "edit-clear-log":
      $("console").innerHTML = "";
      logLine("system", `[${ts}]  ◆  Log cleared.`);
      break;

    case "view-log-quiet":  $("verbosity").value = "0"; syncVerbosityCheck("view-log-quiet");  logLine("system", `[${ts}]  ◆  Log: Quiet`);  break;
    case "view-log-info":   $("verbosity").value = "1"; syncVerbosityCheck("view-log-info");   logLine("system", `[${ts}]  ◆  Log: Info`);   break;
    case "view-log-debug":  $("verbosity").value = "2"; syncVerbosityCheck("view-log-debug");  logLine("system", `[${ts}]  ◆  Log: Debug`);  break;
    case "view-open-output": logLine("system", `[${ts}]  ◆  Open Output Folder — coming soon.`); break;
    case "view-open-log":    logLine("system", `[${ts}]  ◆  Open Session Log — coming soon.`);   break;

    case "preset-save":        openSavePresetModal(); break;
    case "preset-open-folder": openPresetsFolder(); break;
    case "preset-reload":      loadPresetsMenu(); break;
    case "help-docs":    openModal("https://schwwaaa.github.io/AdSlicer-docs/", "Documentation"); break;
    case "help-faq":
    case "help-tips":    openModal("https://schwwaaa.github.io/AdSlicer-docs/", "Tips & Tricks"); break;
    case "help-usecases": openModal("https://schwwaaa.github.io/AdSlicer-docs/use-cases/", "Use Cases"); break;
    case "help-about":   showAbout(); break;
    default: break;
  }
}

function resetParameters() {
  $("blackMinDur").value   = "0.10";
  $("pixTh").value         = "0.08";
  $("picTh").value         = "0.98";
  $("mergeGap").value      = "1.5";
  $("edgePadPre").value    = "0.2";
  $("edgePadPost").value   = "0.06";
  $("minCommercial").value = "5";
  $("maxCommercial").value = "240";
  $("includeBlack").checked = false;
  $("dryRun").checked       = false;
  $("previewDur").value     = "0";
  $("outputMode").value     = "cut";
  $("encodeMode").value     = "copy";
  $("gpuAccel").value       = "none";
  $("videoCrf").value       = "18";
  $("videoPreset").value    = "veryfast";
  $("audioCodec").value     = "aac";
  $("audioBitrateKbps").value = "0";
  $("loudnorm").checked     = false;
  $("deinterlace").checked  = false;
  $("scaleWidth").value     = "0";
  $("verbosity").value      = "2";
  $("globPattern").value    = "*.mp4,*.mov,*.mkv,*.avi,*.m4v,*.wmv,*.flv,*.webm,*.mpg,*.mpeg,*.mts,*.m2ts,*.ts,*.vob,*.3gp,*.dv";
  // ── Comskip-derived enhancements ───────────────────────────────
  $("silenceNoiseDb").value  = "-40";
  $("silenceMinDur").value   = "0.5";
  $("minShowSegment").value  = "30";
  $("alwaysKeepFirst").value = "0";
  $("alwaysKeepLast").value  = "0";
  $("uniformMaxStddev").value = "8";
  $("sceneThreshold").value  = "0.4";
  $("removeBefore").value    = "0";
  $("removeAfter").value     = "0";
  $("requireDiv5").checked   = false;
  $("trimHead").value         = "0";
  $("trimTail").value         = "0";
  $("postCommand").value      = "";
}

function syncVerbosityCheck(active) {
  ["view-log-quiet","view-log-info","view-log-debug"].forEach(a => {
    const el = document.querySelector(`[data-action="${a}"] .dd-check`);
    if (el) el.textContent = (a === active) ? "✓" : "";
  });
}

function showAbout() {
  logBlank();
  logLine("header", "  AdSlicer  v0.1.0");
  logLine("system", "  AdSlicer");
  logLine("system", "  Broadcast Archival Commercial Slicer");
  logLine("system", "  ─────────────────────────────────────");
  logLine("system", "  Cuts commercials from VHS + broadcast");
  logLine("system", "  captures using black-frame detection.");
  logLine("system", "  Built with Tauri · Rust · ffmpeg");
  logBlank();
}


/* ═══════════════════════════════════════════════════════════
   PRESET SYSTEM
   Presets are .json files in {app_config_dir}/presets/.
   The folder is created on first launch and seeded with the
   three built-in presets (default, vhs_noisy, broadcast_strict).
   Users can drop additional .json files there at any time and
   reload via Presets → Reload Presets.
   ═══════════════════════════════════════════════════════════ */

// All param keys expected in a preset JSON (camelCase, matches collectParams)
const PRESET_PARAM_KEYS = [
  "blackMinDur","pixTh","picTh","mergeGap",
  "edgePadPre","edgePadPost","minCommercial","maxCommercial",
  "includeBlack","dryRun","verbosity","outputMode",
  "silenceNoiseDb","silenceMinDur","minShowSegment",
  "alwaysKeepFirst","alwaysKeepLast",
  "uniformMaxStddev","sceneThreshold",
  "removeBefore","removeAfter","requireDiv5"
];

// Apply a parsed preset object to the UI inputs
function applyPreset(preset) {
  const map = {
    blackMinDur:    ["blackMinDur","value"],
    pixTh:          ["pixTh","value"],
    picTh:          ["picTh","value"],
    mergeGap:       ["mergeGap","value"],
    edgePadPre:     ["edgePadPre","value"],
    edgePadPost:    ["edgePadPost","value"],
    minCommercial:  ["minCommercial","value"],
    maxCommercial:  ["maxCommercial","value"],
    includeBlack:   ["includeBlack","checked"],
    dryRun:         ["dryRun","checked"],
    previewDur:     ["previewDur","value"],
    outputMode:     ["outputMode","value"],
    encodeMode:     ["encodeMode","value"],
    gpuAccel:       ["gpuAccel","value"],
    videoCrf:       ["videoCrf","value"],
    videoPreset:    ["videoPreset","value"],
    audioCodec:     ["audioCodec","value"],
    audioBitrateKbps:["audioBitrateKbps","value"],
    loudnorm:       ["loudnorm","checked"],
    deinterlace:    ["deinterlace","checked"],
    scaleWidth:     ["scaleWidth","value"],
    verbosity:      ["verbosity","value"],
    silenceNoiseDb: ["silenceNoiseDb","value"],
    silenceMinDur:  ["silenceMinDur","value"],
    minShowSegment: ["minShowSegment","value"],
    alwaysKeepFirst:["alwaysKeepFirst","value"],
    alwaysKeepLast: ["alwaysKeepLast","value"],
    uniformMaxStddev:["uniformMaxStddev","value"],
    sceneThreshold: ["sceneThreshold","value"],
    removeBefore:   ["removeBefore","value"],
    removeAfter:    ["removeAfter","value"],
    requireDiv5:    ["requireDiv5","checked"],
    trimHead:       ["trimHead","value"],
    trimTail:       ["trimTail","value"],
    postCommand:    ["postCommand","value"],
  };
  let applied = 0;
  for (const [key, [id, prop]] of Object.entries(map)) {
    if (preset[key] === undefined) continue;
    const el = $(id);
    if (!el) continue;
    if (prop === "checked") {
      el.checked = Boolean(preset[key]);
    } else {
      el.value = String(preset[key]);
    }
    applied++;
  }
  return applied;
}

// Populate the Presets dropdown from both the app bundle and user presets folder.
// Built-in presets (from the bundle) appear first under a "Built-in" header.
// User-imported presets appear below under a "My Presets" header.
async function loadPresetsMenu() {
  if (!(core && core.invoke)) return;
  const dropdown = $("presetsDropdown");
  if (!dropdown) return;

  dropdown.querySelectorAll(".preset-dynamic").forEach(el => el.remove());

  let presets = [];
  try {
    presets = await core.invoke("list_presets");
  } catch (e) {
    logLine("warn", `[${timestamp()}]  ⚠  Could not load presets: ${e}`);
    return;
  }

  const firstStatic = dropdown.querySelector('[data-action="preset-save"]');
  const builtins = presets.filter(p => p.builtin);
  const userOwned = presets.filter(p => !p.builtin);

  function makeItem(p) {
    const item = document.createElement("div");
    item.className = "menu-dd-item preset-dynamic";
    item.dataset.action = "preset-apply";
    item.dataset.presetPath = p.path;
    item.dataset.presetName = p.name;
    item.dataset.presetBuiltin = p.builtin ? "1" : "0";
    item.innerHTML = `<span class="dd-check"></span><span class="dd-text" title="${p.description || ''}">${p.label}</span>`;
    return item;
  }

  function makeHeader(text) {
    const h = document.createElement("div");
    h.className = "menu-dd-item disabled preset-dynamic";
    h.innerHTML = `<span class="dd-check"></span><span class="dd-text" style="font-size:10px;text-transform:uppercase;letter-spacing:0.05em;opacity:0.55;">${text}</span>`;
    return h;
  }

  function makeSep() {
    const s = document.createElement("div");
    s.className = "menu-dd-sep preset-dynamic";
    return s;
  }

  if (builtins.length === 0 && userOwned.length === 0) {
    const empty = document.createElement("div");
    empty.className = "menu-dd-item disabled preset-dynamic";
    empty.innerHTML = '<span class="dd-check"></span><span class="dd-text" style="opacity:0.5">No presets found</span>';
    dropdown.insertBefore(empty, firstStatic);
    dropdown.insertBefore(makeSep(), firstStatic);
    return;
  }

  // Insert user presets section last (closest to bottom)
  if (userOwned.length > 0) {
    dropdown.insertBefore(makeSep(), firstStatic);
    userOwned.forEach(p => dropdown.insertBefore(makeItem(p), firstStatic));
    dropdown.insertBefore(makeHeader("My presets"), firstStatic);
  }

  // Insert built-in presets section first (closest to top)
  if (builtins.length > 0) {
    if (userOwned.length > 0) {
      dropdown.insertBefore(makeSep(), firstStatic.previousSibling ? firstStatic.previousSibling : firstStatic);
    }
    builtins.slice().reverse().forEach(p => {
      const item = makeItem(p);
      dropdown.insertBefore(item, dropdown.querySelector(".preset-dynamic"));
    });
    dropdown.insertBefore(makeHeader("Built-in"), dropdown.querySelector(".preset-dynamic"));
  }

  if (builtins.length > 0 || userOwned.length > 0) {
    dropdown.insertBefore(makeSep(), firstStatic);
  }
}

// Apply a preset from the menu (called when a .preset-dynamic item is clicked)
async function applyPresetFromPath(path, name) {
  if (!(core && core.invoke)) return;
  const ts = timestamp();
  try {
    const raw = await core.invoke("load_preset", { path });
    const preset = JSON.parse(raw);
    const n = applyPreset(preset);
    const label = preset._preset || name;
    logBlank();
    logLine("success", `[${ts}]  ✔  Preset loaded: ${label}  (${n} parameter(s) applied)`);
    if (preset._description) {
      logLine("system",  `[${ts}]  ◆  ${preset._description}`);
    }
    logBlank();
  } catch (e) {
    logLine("error", `[${ts}]  ✖  Failed to load preset: ${e}`);
  }
}

// Browse for a .json file and load it as a preset (File → Load Preset…)
async function pickAndLoadPreset() {
  if (!dialog) return;
  const sel = await dialog.open({
    filters: [{ name: "JSON Preset", extensions: ["json"] }],
    multiple: false
  });
  if (!sel) return;
  await applyPresetFromPath(sel, sel.split(/[\/]/).pop().replace(/\.json$/i,""));
}

// Save current UI state as a named preset
function openSavePresetModal() {
  const modal = $("save-preset-modal");
  const input = $("save-preset-name");
  if (!modal || !input) return;
  input.value = "";
  modal.style.display = "flex";
  setTimeout(() => input.focus(), 50);
}

function closeSavePresetModal() {
  const modal = $("save-preset-modal");
  if (modal) modal.style.display = "none";
}

async function confirmSavePreset() {
  if (!(core && core.invoke)) return;
  const nameInput = $("save-preset-name");
  const filename = nameInput ? nameInput.value.trim() : "";
  if (!filename) {
    logLine("warn", `[${timestamp()}]  ⚠  Preset name cannot be empty.`);
    return;
  }
  closeSavePresetModal();
  const ts = timestamp();
  const params = collectParams();
  // Add label from the filename
  params._preset = filename.replace(/[_-]/g, " ");

  try {
    const savedPath = await core.invoke("save_preset", {
      filename,
      content: JSON.stringify(params, null, 2)
    });
    logLine("success", `[${ts}]  ✔  Preset saved: ${filename}.json`);
    logLine("system",  `[${ts}]  ◆  ${savedPath}`);
    // Refresh the presets menu so the new one appears immediately
    await loadPresetsMenu();
  } catch (e) {
    logLine("error", `[${ts}]  ✖  Failed to save preset: ${e}`);
  }
}

// Open the presets folder in the OS file manager
async function openPresetsFolder() {
  if (!(core && core.invoke)) return;
  const ts = timestamp();
  try {
    const dir = await core.invoke("get_presets_dir");
    // Use Tauri shell open — falls back gracefully if unavailable
    if (T.shell && T.shell.open) {
      await T.shell.open(dir);
    } else {
      logLine("system", `[${ts}]  ◆  User presets folder: ${dir}`);
    logLine("system", `[${ts}]  ◆  Drop .json files here and use Reload Presets to add them.`);
    }
  } catch (e) {
    logLine("warn", `[${ts}]  ⚠  Could not open presets folder: ${e}`);
  }
}

/* ─── Init ─── */
window.addEventListener("DOMContentLoaded", () => {
  $("pickInput").onclick  = pickInput;
  $("pickOutput").onclick = pickOutput;
  $("startBtn").onclick   = startJob;
  $("stopBtn").onclick    = stopJob;

  $("clearConsole").onclick = () => {
    $("console").innerHTML = "";
    logLine("system", `[${timestamp()}]  ◆  Log cleared.`);
  };

  document.querySelectorAll('input[name="mode"]').forEach(r => {
    r.addEventListener("change", updateInputLabel);
  });

  $("inputPath").addEventListener("input", () => {
    const v = $("inputPath").value;
    $("sb-file").textContent = v ? truncatePath(v) : "No file selected";
  });

// Backend log stream (Tauri event plugin). On some systems the first run can race
// the plugin init; make it retry + never allow an unhandled rejection.
async function setupBackendLogStream() {
  if (!(event && event.listen)) return;
  for (let attempt = 1; attempt <= 5; attempt++) {
    try {
      await event.listen("adslicer-log", (m) => {
        const payload = m.payload || "";
        if (/\[done\]|job complete/i.test(payload)) {
          parseAndLog(payload);
          setStatus("done", "Complete");
          jobRunning = false;
          return;
        }
        if (/^\[error\]/i.test(payload)) {
          parseAndLog(payload);
          setStatus("error", "Error");
          jobRunning = false;
          return;
        }
        parseAndLog(payload);
      });
      return;
    } catch (err) {
      logLine("warn", `[${timestamp()}]  ⚠  Log stream not ready (attempt ${attempt}/5) — retrying…`);
      await new Promise(r => setTimeout(r, 200));
    }
  }
  logLine("warn", `[${timestamp()}]  ⚠  Log stream unavailable; UI will still work.`);
}
setupBackendLogStream();

  // Logo nav links — open in modal
  document.querySelector('.logo-links').addEventListener('click', (e) => {
    const a = e.target.closest('a.link');
    if (!a) return;
    e.preventDefault();
    openModal(a.href, a.textContent.trim());
  });

  // Modal close button
  document.getElementById("web-modal-close").addEventListener("click", closeModal);

  // Click backdrop (outside inner box) to close
  document.getElementById("web-modal").addEventListener("click", (e) => {
    if (e.target === document.getElementById("web-modal")) closeModal();
  });

  // Escape key closes modal
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && document.getElementById("web-modal").style.display !== "none") {
      closeModal();
    }
  });

  initTooltips();
  initMenuBar();

  // ── Preset system init ────────────────────────────────────────────────
  // Populate the presets menu from the folder on startup
  loadPresetsMenu();

  // Delegate preset-apply clicks (items are injected dynamically)
  const presetsDropdown = $("presetsDropdown");
  if (presetsDropdown) {
    presetsDropdown.addEventListener("click", async (e) => {
      const item = e.target.closest("[data-action='preset-apply']");
      if (!item) return;
      await applyPresetFromPath(item.dataset.presetPath, item.dataset.presetName);
    });
  }

  // Save preset modal wiring
  const spClose   = $("save-preset-close");
  const spCancel  = $("save-preset-cancel");
  const spConfirm = $("save-preset-confirm");
  const spName    = $("save-preset-name");
  if (spClose)   spClose.addEventListener("click",   closeSavePresetModal);
  if (spCancel)  spCancel.addEventListener("click",  closeSavePresetModal);
  if (spConfirm) spConfirm.addEventListener("click", confirmSavePreset);
  if (spName)    spName.addEventListener("keydown",  (e) => { if (e.key === "Enter") confirmSavePreset(); });
  const spModal = $("save-preset-modal");
  if (spModal) spModal.addEventListener("click", (e) => {
    if (e.target === spModal) closeSavePresetModal();
  });

  // Startup banner
  logLine("system", "╔═══════════════════════════════════════════╗");
  logLine("system", "║   AdSlicer  v0.1.0                   ║");
  logLine("system", "║   Broadcast Archival Commercial Slicer     ║");
  logLine("system", "╚═══════════════════════════════════════════╝");
  logBlank();
  logLine("system", `[${timestamp()}]  ◆  Ready. Configure parameters and press Start.`);
  logBlank();
});
