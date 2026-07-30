import re, subprocess, logging
from typing import List, Tuple
from .models import BlackSeg

LOG = logging.getLogger("blackcut.detect")

BL_START = re.compile(r'black_start:(?P<start>[0-9.]+)')
BL_END   = re.compile(r'black_end:(?P<end>[0-9.]+)')

# Use list-form args (no shell=True, no shlex.quote) so paths with spaces
# and Windows backslashes work correctly on all platforms.

def probe_duration(path: str) -> float:
    cmd = [
        "ffprobe",
        "-v", "error",
        "-show_entries", "format=duration",
        "-of", "default=noprint_wrappers=1:nokey=1",
        path,
    ]
    out = subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL).strip()
    return float(out)

def run_blackdetect(
    path: str,
    black_min_dur: float = 0.1,
    pix_th: float = 0.10,
    pic_th: float = 0.98,
    clog=None,
) -> Tuple[List[BlackSeg], float, str]:
    """Returns (blacks, duration, raw_ffmpeg_stderr). Uses ffmpeg blackdetect filter."""
    vf  = f'blackdetect=d={black_min_dur}:pix_th={pix_th}:pic_th={pic_th}'
    cmd = [
        "ffmpeg",
        "-hide_banner", "-nostats",
        "-i", path,
        "-vf", vf,
        "-f", "null", "-",
    ]
    LOG.debug("FFmpeg detect argv: %s", cmd)

    proc = subprocess.run(
        cmd,
        stderr=subprocess.PIPE,
        stdout=subprocess.DEVNULL,
        text=True,
    )
    stderr = proc.stderr or ""

    blacks: List[BlackSeg] = []
    cur_start = None
    for line in stderr.splitlines():
        if "black_start" in line or "black_end" in line:
            LOG.debug("[blackdetect] %s", line)
        m1 = BL_START.search(line)
        if m1:
            cur_start = float(m1.group("start"))
        m2 = BL_END.search(line)
        if m2 and cur_start is not None:
            blacks.append(BlackSeg(start=cur_start, end=float(m2.group("end"))))
            cur_start = None

    duration = probe_duration(path)
    LOG.info("ffprobe duration: %.3fs", duration)
    LOG.info("blackdetect found %d segments (pre-merge)", len(blacks))
    return blacks, duration, stderr

def merge_close(blacks: List[BlackSeg], gap: float) -> List[BlackSeg]:
    if not blacks: return []
    blacks = sorted(blacks, key=lambda b: b.start)
    merged = [blacks[0]]
    for b in blacks[1:]:
        last = merged[-1]
        if b.start - last.end <= gap:
            merged[-1] = BlackSeg(start=last.start, end=max(last.end, b.end))
        else:
            merged.append(b)
    LOG.debug("Merged to %d segments with gap<=%.3fs", len(merged), gap)
    return merged

def drop_too_short(blacks: List[BlackSeg], min_dur: float) -> List[BlackSeg]:
    kept = [b for b in blacks if b.dur >= min_dur]
    LOG.debug("Dropped %d segments < %.3fs", len(blacks) - len(kept), min_dur)
    return kept
