import argparse, os, pathlib, logging, sys, io
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
from .detect import run_blackdetect, merge_close, drop_too_short
from .cut import build_plan, write_logs, export_commercials, export_show, format_ts

LOG = logging.getLogger("AdSlicerProXP")

def setup_logging(verbosity: int):
    level = logging.WARNING
    if verbosity == 1:
        level = logging.INFO
    elif verbosity >= 2:
        level = logging.DEBUG
    logging.basicConfig(
        level=level,
        format="%(levelname).1s %(name)s | %(message)s",
        stream=sys.stdout
    )

def print_plan_summary(outdir: str, base: str, duration: float, plan) -> None:
    LOG.info("-- Plan for %s (duration %s) --", base, format_ts(duration))
    if not plan.commercials:
        LOG.info("No commercials detected.")
    else:
        LOG.info("Commercials (%d):", len(plan.commercials))
        print(" idx |     start      ->        end       |   dur  | kind")
        print("-----+----------------+------------------+--------+------")
        for i, c in enumerate(plan.commercials, 1):
            print(f"{i:>3} | {format_ts(c.start):>12} -> {format_ts(c.end):>12} | {format_ts(c.dur):>6} | {c.kind}")
    LOG.info("Keeps (%d total segments)", len(plan.keeps))
    # Print just the first/last few keeps to keep output tidy
    keep_to_show = 4
    for i, k in enumerate(plan.keeps[:keep_to_show], 1):
        LOG.debug("KEEP %02d %s -> %s (%s)", i, format_ts(k.start), format_ts(k.end), format_ts(k.dur))
    if len(plan.keeps) > keep_to_show:
        LOG.debug("... %d more keep segments not shown", len(plan.keeps) - keep_to_show)
    LOG.info("Edge pads: pre=%.3fs post=%.3fs | include_black=%s",
             plan.edge_pad_pre, plan.edge_pad_post, plan.include_black)

def process_one(
    input_path: str,
    outdir_root: str,
    black_min_dur: float,
    pix_th: float,
    pic_th: float,
    merge_gap: float,
    edge_pad_pre: float,
    edge_pad_post: float,
    min_commercial: float,
    max_commercial: float,
    include_black: bool,
    reencode: bool,
    dry_run: bool,
    verbosity: int
) -> None:
    base = pathlib.Path(input_path).stem
    outdir = os.path.join(outdir_root, base)
    os.makedirs(outdir, exist_ok=True)

    LOG.info("Analyzing: %s", input_path)
    blacks, duration, raw_ff = run_blackdetect(
        input_path,
        black_min_dur=black_min_dur,
        pix_th=pix_th,
        pic_th=pic_th
    )

    # Save raw ffmpeg blackdetect stderr if verbose
    if verbosity >= 1:
        logs_dir = os.path.join(outdir, "logs")
        os.makedirs(logs_dir, exist_ok=True)
        with open(os.path.join(logs_dir, "ffmpeg_blackdetect.log"), "w") as f:
            f.write(raw_ff or "")

    LOG.info("Detected %d raw black segments", len(blacks))
    blacks = drop_too_short(blacks, black_min_dur)
    blacks = merge_close(blacks, merge_gap)
    LOG.info("After filtering/merge: %d black segments", len(blacks))

    plan = build_plan(
        blacks=blacks,
        duration=duration,
        include_black=include_black,
        edge_pad_pre=edge_pad_pre,
        edge_pad_post=edge_pad_post,
        min_commercial=min_commercial,
        max_commercial=max_commercial
    )

    # Log + write plan
    print_plan_summary(outdir, base, duration, plan)
    write_logs(outdir, base, plan)

    if dry_run:
        LOG.info("[dry-run] Logs only -> %s", os.path.join(outdir, "logs"))
        return

    com_paths = export_commercials(input_path, outdir, base, plan, reencode=reencode)
    show_path = export_show(input_path, outdir, base, plan, reencode=reencode)

    LOG.info("DONE Done: %s", input_path)
    LOG.info("Commercial files: %d -> %s", len(com_paths), os.path.join(outdir, "commercials"))
    LOG.info("Show file: %s", show_path)
    LOG.info("Logs: %s", os.path.join(outdir, "logs"))

VIDEO_EXTENSIONS = {
    ".mp4", ".m4v", ".mov", ".avi", ".mkv", ".wmv", ".flv", ".webm",
    ".mpeg", ".mpg", ".mts", ".m2ts", ".ts", ".vob", ".ogv", ".3gp",
    ".3g2", ".divx", ".xvid", ".rmvb", ".rm", ".asf", ".f4v", ".dv",
}

def iter_inputs(folder: str, glob_pattern: str):
    import glob as g
    seen = set()
    # Support comma-separated patterns e.g. "*.mp4,*.mov,*.mkv"
    patterns = [p.strip() for p in glob_pattern.split(",") if p.strip()]
    for pattern in patterns:
        for path in g.glob(os.path.join(folder, '**', pattern), recursive=True):
            if os.path.isfile(path) and path not in seen:
                seen.add(path)
                yield path
        # Also match uppercase extensions on case-sensitive filesystems (e.g. *.MP4)
        if pattern.startswith("*."):
            upper = "*." + pattern[2:].upper()
            if upper != pattern:
                for path in g.glob(os.path.join(folder, '**', upper), recursive=True):
                    if os.path.isfile(path) and path not in seen:
                        seen.add(path)
                        yield path

def main(argv=None):
    p = argparse.ArgumentParser(description="Detect black slugs and cut commercials from VHS captures")
    src = p.add_mutually_exclusive_group(required=True)
    src.add_argument("-i", "--input", help="Single input video file")
    src.add_argument("-I", "--input-dir", help="Folder of videos (recursively)")

    p.add_argument("--glob", default="*.mp4,*.mov,*.mkv,*.avi,*.m4v,*.wmv,*.flv,*.webm,*.mpg,*.mpeg,*.mts,*.m2ts,*.ts,*.vob,*.3gp,*.dv", help="Comma-separated glob patterns for batch mode")
    p.add_argument("--outdir", required=True, help="Output root directory")

    # detection
    p.add_argument("--black-min-dur", type=float, default=0.10, help="Minimum black duration (seconds)")
    p.add_argument("--pix-th", type=float, default=0.08, help="Pixel threshold for black (0-1)")
    p.add_argument("--pic-th", type=float, default=0.98, help="Frame black pixel fraction threshold (0-1)")
    p.add_argument("--merge-gap", type=float, default=1.5, help="Merge consecutive blacks with gap <= this (seconds)")

    # cutting behavior
    p.add_argument("--edge-pad-pre", type=float, default=0.20, help="Padding before cut (seconds)")
    p.add_argument("--edge-pad-post", type=float, default=0.06, help="Padding after cut (seconds)")
    p.add_argument("--min-commercial", type=float, default=5.0, help="Minimum interval between blacks to treat as ad")
    p.add_argument("--max-commercial", type=float, default=240.0, help="Maximum interval between blacks to treat as ad")
    p.add_argument("--include-black", action="store_true", help="Include bounding black segments in exported commercials")
    p.add_argument("--reencode", action="store_true", help="Re-encode for accurate frame-edge cuts")
    p.add_argument("--dry-run", action="store_true", help="Only write logs (no media editing)")

    # verbosity
    p.add_argument("-v", "--verbose", action="count", default=0,
                   help="Increase verbosity: -v (INFO), -vv (DEBUG)")

    args = p.parse_args(argv)
    setup_logging(args.verbose)

    if args.input:
        process_one(
            args.input, args.outdir,
            args.black_min_dur, args.pix_th, args.pic_th, args.merge_gap,
            args.edge_pad_pre, args.edge_pad_post,
            args.min_commercial, args.max_commercial,
            args.include_black, args.reencode, args.dry_run,
            args.verbose
        )
    else:
        count = 0
        for f in iter_inputs(args.input_dir, args.glob):
            count += 1
            LOG.info("- Processing %s", f)
            try:
                process_one(
                    f, args.outdir,
                    args.black_min_dur, args.pix_th, args.pic_th, args.merge_gap,
                    args.edge_pad_pre, args.edge_pad_post,
                    args.min_commercial, args.max_commercial,
                    args.include_black, args.reencode, args.dry_run,
                    args.verbose
                )
            except Exception as e:
                LOG.exception("Failed on %s: %s", f, e)
        LOG.info("Done. Processed %d files.", count)

if __name__ == '__main__':
    main()
