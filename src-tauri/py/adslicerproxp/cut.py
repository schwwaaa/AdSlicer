import csv, json, os, subprocess, logging, math, shutil
from typing import List
from .models import BlackSeg, CutInterval, Plan

LOG = logging.getLogger("blackcut.cut")

def format_ts(t: float) -> str:
    if t < 0: t = 0
    ms = int(round((t - math.floor(t)) * 1000))
    s  = int(t) % 60
    m  = (int(t) // 60) % 60
    h  = int(t) // 3600
    return f"{h:02d}:{m:02d}:{s:02d}.{ms:03d}"

def _safe_out(path: str) -> str:
    """Return a non-colliding output path."""
    base, ext = os.path.splitext(path)
    i = 1
    out = path
    while os.path.exists(out):
        out = f"{base}_{i}{ext}"
        i += 1
    return out

def build_plan(
    blacks: List[BlackSeg],
    duration: float,
    include_black: bool,
    edge_pad_pre: float,
    edge_pad_post: float,
    min_commercial: float,
    max_commercial: float
) -> Plan:
    commercials: List[CutInterval] = []
    for i in range(len(blacks) - 1):
        left  = blacks[i]
        right = blacks[i + 1]
        start = left.start  if include_black else left.end
        end   = right.end   if include_black else right.start
        start = max(0.0, start - edge_pad_pre)
        end   = min(duration, end + edge_pad_post)
        if end > start:
            dur = end - start
            if min_commercial <= dur <= max_commercial:
                commercials.append(CutInterval(start=start, end=end, kind="commercial"))

    keeps: List[CutInterval] = []
    cursor = 0.0
    for c in commercials:
        if c.start > cursor:
            keeps.append(CutInterval(start=cursor, end=c.start, kind="keep"))
        cursor = c.end
    if cursor < duration:
        keeps.append(CutInterval(start=cursor, end=duration, kind="keep"))

    LOG.info("Plan: %d commercials, %d keeps", len(commercials), len(keeps))
    return Plan(
        blacks=blacks, commercials=commercials, keeps=keeps,
        include_black=include_black,
        edge_pad_pre=edge_pad_pre, edge_pad_post=edge_pad_post,
        params={"min_commercial": min_commercial, "max_commercial": max_commercial},
        duration=duration,
    )

def write_logs(outdir: str, base: str, plan: Plan) -> None:
    os.makedirs(os.path.join(outdir, "logs"), exist_ok=True)
    jpath = os.path.join(outdir, "logs", "detect.json")
    cpath = os.path.join(outdir, "logs", "detect.csv")
    with open(jpath, "w") as f:
        json.dump({
            "duration": plan.duration,
            "params": {"include_black": plan.include_black,
                       "edge_pad_pre": plan.edge_pad_pre,
                       "edge_pad_post": plan.edge_pad_post,
                       **plan.params},
            "blacks":      [vars(b) for b in plan.blacks],
            "commercials": [vars(c) for c in plan.commercials],
            "keeps":       [vars(k) for k in plan.keeps],
        }, f, indent=2)
    with open(cpath, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["type", "start", "end", "dur"])
        for b in plan.blacks:
            w.writerow(["black",      f"{b.start:.3f}", f"{b.end:.3f}", f"{b.dur:.3f}"])
        for c in plan.commercials:
            w.writerow(["commercial", f"{c.start:.3f}", f"{c.end:.3f}", f"{c.dur:.3f}"])
        for k in plan.keeps:
            w.writerow(["keep",       f"{k.start:.3f}", f"{k.end:.3f}", f"{k.dur:.3f}"])
    LOG.info("Wrote logs: %s", os.path.join(outdir, "logs"))

def _run_ffmpeg(args: List[str]) -> None:
    """Run ffmpeg given as a list of arguments — no shell, no quoting issues."""
    LOG.debug("FFmpeg: %s", args)
    subprocess.check_call(args)

def ffmpeg_cut(input_path: str, start: float, end: float,
               out_path: str, reencode: bool = False) -> None:
    dur = max(0.0, end - start)
    if dur <= 0:
        LOG.debug("Skip empty cut at %s", format_ts(start))
        return
    if not reencode:
        args = [
            "ffmpeg", "-y", "-hide_banner", "-nostats",
            "-ss", f"{start:.3f}",
            "-i", input_path,
            "-t", f"{dur:.3f}",
            "-c", "copy",
            out_path,
        ]
    else:
        args = [
            "ffmpeg", "-y", "-hide_banner", "-nostats",
            "-ss", f"{start:.3f}",
            "-i", input_path,
            "-t", f"{dur:.3f}",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "18",
            "-c:a", "aac", "-movflags", "+faststart",
            out_path,
        ]
    _run_ffmpeg(args)

def export_commercials(input_path: str, outdir: str, base: str,
                       plan: Plan, reencode: bool = False):
    os.makedirs(os.path.join(outdir, "commercials"), exist_ok=True)
    outs = []
    for i, c in enumerate(plan.commercials, 1):
        outp = _safe_out(os.path.join(outdir, "commercials", f"{base}_ad_{i:04d}.mp4"))
        LOG.info("Cut COMM %02d: %s -> %s (%s) -> %s",
                 i, format_ts(c.start), format_ts(c.end), format_ts(c.dur), outp)
        ffmpeg_cut(input_path, c.start, c.end, outp, reencode=reencode)
        outs.append(outp)
    return outs

def export_show(input_path: str, outdir: str, base: str,
                plan: Plan, reencode: bool = False) -> str:
    os.makedirs(os.path.join(outdir, "show"), exist_ok=True)
    tmpdir = os.path.join(outdir, "show", "_parts")
    os.makedirs(tmpdir, exist_ok=True)

    parts = []
    for i, k in enumerate(plan.keeps, 1):
        part = os.path.join(tmpdir, f"part_{i:04d}.mp4")
        LOG.info("Cut KEEP %02d: %s -> %s (%s) -> %s",
                 i, format_ts(k.start), format_ts(k.end), format_ts(k.dur), part)
        ffmpeg_cut(input_path, k.start, k.end, part, reencode=reencode)
        parts.append(part)

    outp = _safe_out(os.path.join(outdir, "show", f"{base}_show.mp4"))

    if len(parts) == 0:
        LOG.warning("No KEEP segments -> skipping show export.")
        return outp

    if len(parts) == 1:
        LOG.info("Single KEEP segment -> using it directly as show output.")
        shutil.copy2(parts[0], outp)
        return outp

    list_txt = os.path.join(tmpdir, "list.txt")
    with open(list_txt, "w") as f:
        for p in parts:
            # ffmpeg concat list requires forward slashes even on Windows
            ap = os.path.abspath(p).replace("\\", "/")
            f.write(f"file '{ap}'\n")

    if not reencode:
        args = ["ffmpeg", "-y", "-hide_banner", "-nostats",
                "-f", "concat", "-safe", "0",
                "-i", list_txt, "-c", "copy", outp]
    else:
        args = ["ffmpeg", "-y", "-hide_banner", "-nostats",
                "-f", "concat", "-safe", "0", "-i", list_txt,
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "18",
                "-c:a", "aac", "-movflags", "+faststart", outp]

    LOG.info("Concat show -> %s", outp)
    _run_ffmpeg(args)
    return outp
