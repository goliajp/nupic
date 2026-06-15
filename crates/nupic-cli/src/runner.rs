use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use nupic_core::{
    AlphaBboxOpts, CircleOpts, Color, CompressOpts, CropOpts, DenoiseOpts, FilterOpts, FitOpts,
    Font, Format, Image, Metric, MockOpts, MockStyle, PerceptualTarget, Quality, Rect, ResizeMode,
    ResizeOpts, Size, WatermarkContent, WatermarkOpts, alpha_bbox, metrics,
};

use clap::CommandFactory;

use crate::cli::{
    BboxArgs, BenchArgs, Cli, CircleArgs, Command, CommonIo, CompareArgs, CompletionsArgs,
    CompressArgs, CropArgs, DenoiseArgs, FilterArgs, FitArgs, MockArgs, MockStyleArg, ResizeArgs,
    WatermarkArgs,
};

pub fn run(args: Cli) -> Result<()> {
    let _ = args.verbose; // wired but not yet routed to a logger
    match args.command {
        Command::Resize(args) => run_resize(args),
        Command::Fit(args) => run_fit(args),
        Command::Circle(args) => run_circle(args),
        Command::Mock(args) => run_mock(args),
        Command::Watermark(args) => run_watermark(args),
        Command::Compress(args) => run_compress(args),
        Command::Compare(args) => run_compare(args),
        Command::Crop(args) => run_crop(args),
        Command::Filter(args) => run_filter(args),
        Command::Denoise(args) => run_denoise(args),
        Command::Bbox(args) => run_bbox(args),
        Command::Completions(args) => run_completions(args),
        Command::Bench(args) => run_bench(args),
    }
}

#[derive(serde::Deserialize)]
struct BaselineFile {
    fixtures: std::collections::BTreeMap<String, BaselineRow>,
}

#[derive(serde::Deserialize)]
struct BaselineRow {
    tinypng_bytes: u64,
}

fn run_bench(args: BenchArgs) -> Result<()> {
    if args.baseline.is_some() {
        return run_bench_vs_baseline(args);
    }
    let formats = parse_formats(&args.formats)?;

    let mut inputs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&args.dataset)
        .with_context(|| format!("could not read dataset directory {}", args.dataset.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && Format::from_path(&path).is_some() {
            inputs.push(path);
        }
    }
    inputs.sort();
    inputs.truncate(args.limit);
    if inputs.is_empty() {
        return Err(anyhow!(
            "no image files found in {}",
            args.dataset.display()
        ));
    }

    println!(
        "Benchmarking {} image(s) across {} format(s) (effort={}). DSSIM lower = better.",
        inputs.len(),
        formats.len(),
        args.effort
    );
    println!();
    println!(
        "{:<32}  {:<6}  {:>10}  {:>10}  {:>10}",
        "input", "format", "size_b", "encode_ms", "DSSIM"
    );
    println!("{:-<32}  {:-<6}  {:->10}  {:->10}  {:->10}", "", "", "", "", "");

    // Accumulator: parallel to `formats` order, so we always print in the
    // user-requested order.
    let mut totals: Vec<(u64, u64, f64, usize)> = vec![(0, 0, 0.0, 0); formats.len()];

    for input in &inputs {
        let img = decode_input(input)?;
        let name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let trunc_name = truncate_left(&name, 32);

        for (fmt_idx, &fmt) in formats.iter().enumerate() {
            let opts = CompressOpts {
                format: fmt,
                quality: Quality::Auto,
                strip_metadata: false,
                effort: args.effort,
            };
            let t0 = std::time::Instant::now();
            let encoded = match img.compress(opts) {
                Ok(e) => e,
                Err(e) => {
                    println!(
                        "{trunc_name:<32}  {:<6}  {:>10}  {:>10}  {:>10}    [{e}]",
                        format_short(fmt),
                        "—",
                        "—",
                        "—"
                    );
                    continue;
                }
            };
            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let size = encoded.bytes.len();
            // For DSSIM, decode the encoded bytes back. Skip if encoded format
            // isn't decodable by us (AVIF currently).
            let dssim = Image::decode(&encoded.bytes)
                .ok()
                .and_then(|distorted| metrics::compute(Metric::Dssim, &img, &distorted).ok());
            let dssim_display = dssim
                .map(|d| format!("{d:.5}"))
                .unwrap_or_else(|| "—".to_string());

            println!(
                "{trunc_name:<32}  {:<6}  {size:>10}  {elapsed_ms:>10.2}  {dssim_display:>10}",
                format_short(fmt),
            );

            let entry = &mut totals[fmt_idx];
            entry.0 += size as u64;
            entry.1 += (elapsed_ms as u64).max(1);
            if let Some(d) = dssim {
                entry.2 += d;
            }
            entry.3 += 1;
        }
    }

    println!();
    println!("Averages:");
    println!(
        "{:<32}  {:<6}  {:>10}  {:>10}  {:>10}",
        "", "format", "size_b", "encode_ms", "DSSIM"
    );
    println!("{:-<32}  {:-<6}  {:->10}  {:->10}  {:->10}", "", "", "", "", "");
    for (i, &fmt) in formats.iter().enumerate() {
        let (total_size, total_ms, total_dssim, n) = totals[i];
        if n == 0 {
            continue;
        }
        let avg_size = total_size / n as u64;
        let avg_ms = total_ms / n as u64;
        let avg_dssim = format!("{:.5}", total_dssim / n as f64);
        println!(
            "{:<32}  {:<6}  {avg_size:>10}  {avg_ms:>10}  {avg_dssim:>10}",
            "",
            format_short(fmt),
        );
    }
    Ok(())
}

/// PNG-only bench mode that compares `nupic compress` against a pinned
/// external baseline (e.g. TinyPNG byte sizes captured into a JSON file).
/// Exits non-zero if any input regresses past 1.15x the baseline.
fn run_bench_vs_baseline(args: BenchArgs) -> Result<()> {
    let baseline_path = args.baseline.as_ref().expect("checked by caller");
    let baseline_text = fs::read_to_string(baseline_path)
        .with_context(|| format!("could not read baseline {}", baseline_path.display()))?;
    let baseline: BaselineFile = serde_json::from_str(&baseline_text)
        .with_context(|| format!("baseline {} is not valid JSON", baseline_path.display()))?;

    let mut inputs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&args.dataset)
        .with_context(|| format!("could not read dataset directory {}", args.dataset.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && matches!(Format::from_path(&path), Some(Format::Png)) {
            inputs.push(path);
        }
    }
    inputs.sort();
    inputs.truncate(args.limit);
    if inputs.is_empty() {
        return Err(anyhow!(
            "no PNG files found in {}",
            args.dataset.display()
        ));
    }

    println!(
        "PNG bench vs baseline {} ({} input(s), effort={}). Pass = nupic <= 1.15x tinypng.",
        baseline_path.display(),
        inputs.len(),
        args.effort,
    );
    println!();
    println!(
        "{:<32}  {:>10}  {:>10}  {:>10}  {:>8}  {:>4}",
        "input", "input_b", "nupic_b", "tinypng_b", "ratio", "ok?"
    );
    println!(
        "{:-<32}  {:->10}  {:->10}  {:->10}  {:->8}  {:->4}",
        "", "", "", "", "", ""
    );

    let mut total_input = 0u64;
    let mut total_nupic = 0u64;
    let mut total_tinypng = 0u64;
    let mut failures = 0usize;

    for input in &inputs {
        let name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let trunc_name = truncate_left(&name, 32);
        let input_bytes = fs::metadata(input)?.len();
        let baseline_row = baseline.fixtures.get(&name);

        let img = decode_input(input)?;
        let opts = CompressOpts {
            format: Format::Png,
            quality: Quality::Auto,
            strip_metadata: false,
            effort: args.effort,
        };
        let encoded = img.compress(opts)?;
        let nupic_bytes = encoded.bytes.len() as u64;

        let (tinypng_bytes, ratio, ok) = match baseline_row {
            Some(row) => {
                let r = nupic_bytes as f64 / row.tinypng_bytes as f64;
                let ok = r <= 1.15;
                (row.tinypng_bytes, r, ok)
            }
            None => (0, f64::NAN, true), // unmapped inputs don't fail the run
        };
        if !ok {
            failures += 1;
        }
        let ratio_display = if ratio.is_nan() {
            "—".to_string()
        } else {
            format!("{ratio:.3}x")
        };
        let ok_display = if baseline_row.is_none() {
            "—"
        } else if ok {
            "OK"
        } else {
            "FAIL"
        };
        println!(
            "{trunc_name:<32}  {input_bytes:>10}  {nupic_bytes:>10}  {tinypng_bytes:>10}  {ratio_display:>8}  {ok_display:>4}"
        );
        total_input += input_bytes;
        total_nupic += nupic_bytes;
        total_tinypng += tinypng_bytes;
    }

    println!();
    let overall_ratio = if total_tinypng > 0 {
        total_nupic as f64 / total_tinypng as f64
    } else {
        f64::NAN
    };
    println!(
        "TOTAL  input={total_input}  nupic={total_nupic}  tinypng={total_tinypng}  nupic/tinypng={overall_ratio:.3}x"
    );
    if failures > 0 {
        Err(anyhow!(
            "{failures} input(s) exceeded 1.15x the TinyPNG baseline"
        ))
    } else {
        println!("all inputs within 1.15x of TinyPNG baseline.");
        Ok(())
    }
}

fn parse_formats(s: &str) -> Result<Vec<Format>> {
    let mut out = Vec::new();
    for token in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let fmt = match token.to_ascii_lowercase().as_str() {
            "png" => Format::Png,
            "jpeg" | "jpg" => Format::Jpeg,
            "webp" => Format::Webp,
            "avif" => Format::Avif,
            "gif" => Format::Gif,
            "bmp" => Format::Bmp,
            "tiff" => Format::Tiff,
            _ => return Err(anyhow!("unknown bench format: {token}")),
        };
        out.push(fmt);
    }
    if out.is_empty() {
        return Err(anyhow!("at least one format required"));
    }
    Ok(out)
}

fn truncate_left(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        let start = count - max_chars + 1;
        let rest: String = s.chars().skip(start).collect();
        format!("…{rest}")
    }
}

fn format_short(f: Format) -> &'static str {
    match f {
        Format::Png => "png",
        Format::Jpeg => "jpeg",
        Format::Webp => "webp",
        Format::Avif => "avif",
        Format::Gif => "gif",
        Format::Bmp => "bmp",
        Format::Tiff => "tiff",
        Format::Jxl => "jxl",
        Format::Auto => "auto",
        _ => "?",
    }
}

fn run_completions(args: CompletionsArgs) -> Result<()> {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    clap_complete::generate(args.shell, &mut cmd, bin_name, &mut std::io::stdout());
    Ok(())
}

fn run_denoise(args: DenoiseArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let opts = DenoiseOpts::new(args.kind).with_strength(args.strength);
    let result = img.denoise(opts)?;
    write_image_output(&result, &args.io, "denoise")
}

fn run_bbox(args: BboxArgs) -> Result<()> {
    let img = decode_input(&args.input)?;
    let rect = alpha_bbox(
        &img,
        AlphaBboxOpts {
            threshold: args.threshold,
        },
    )?;
    println!(
        "{} {} {} {}",
        rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
    );
    Ok(())
}

fn run_crop(args: CropArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let opts = CropOpts::new(Rect::from_xywh(args.x, args.y, args.width, args.height));
    let result = img.crop(opts)?;
    write_image_output(&result, &args.io, "crop")
}

fn run_filter(args: FilterArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let mut opts = FilterOpts::new(args.kind);
    if let Some(a) = args.amount {
        opts = opts.with_amount(a);
    }
    let result = img.filter(opts)?;
    write_image_output(&result, &args.io, "filter")
}

fn run_compare(args: CompareArgs) -> Result<()> {
    let reference = decode_input(&args.reference)?;
    let distorted = decode_input(&args.distorted)?;
    let value = metrics::compute(args.metric, &reference, &distorted)?;
    let (name, scale_note) = match args.metric {
        Metric::Dssim => ("DSSIM", "lower is better (0 = identical)"),
        Metric::Ssimulacra2 => ("SSIMULACRA2", "higher is better (100 = identical)"),
        Metric::Butteraugli => ("Butteraugli", "lower is better (0 = identical)"),
        _ => ("metric", ""),
    };
    println!("{name}: {value:.6}  ({scale_note})");
    Ok(())
}

// ---------------- subcommand handlers ----------------

fn run_resize(args: ResizeArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let mode = build_resize_mode(&args)?;
    let opts = ResizeOpts::new(mode).with_filter(args.filter);
    let result = img.resize(opts)?;
    write_image_output(&result, &args.io, "resized")
}

fn run_fit(args: FitArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let bg: Color = args
        .bg
        .parse()
        .with_context(|| format!("invalid --bg color: {:?}", args.bg))?;
    let opts = FitOpts::new(Size::new(args.width, args.height), args.mode)
        .with_filter(args.filter)
        .with_background(bg);
    let result = img.fit(opts)?;
    write_image_output(&result, &args.io, "fit")
}

fn run_circle(args: CircleArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let opts = CircleOpts {
        radius: args.radius,
        feather: args.feather,
    };
    let result = img.circle(opts)?;
    write_image_output(&result, &args.io, "circle")
}

fn run_mock(args: MockArgs) -> Result<()> {
    let bg: Color = args
        .bg
        .parse()
        .with_context(|| format!("invalid --bg color: {:?}", args.bg))?;
    let fg: Color = args
        .fg
        .parse()
        .with_context(|| format!("invalid --fg color: {:?}", args.fg))?;
    let style = match args.style {
        MockStyleArg::Stripes => MockStyle::Stripes,
        MockStyleArg::Solid => MockStyle::Solid,
        MockStyleArg::Gradient => MockStyle::Gradient,
        MockStyleArg::Checker => MockStyle::Checker { tile: args.tile },
    };
    let font = match &args.font {
        Some(path) => Font::from_path(path)
            .with_context(|| format!("failed to load font {}", path.display()))?,
        None => Font::default_font(),
    };
    let opts = MockOpts {
        size: Size::new(args.width, args.height),
        style,
        background: bg,
        foreground: fg,
        text: args.text.clone(),
        font,
    };
    let img = nupic_core::ops::mock::render(opts)?;

    let format = if args.format == Format::Auto {
        match args.output.as_deref().and_then(Format::from_path) {
            Some(f) => f,
            None => Format::Png,
        }
    } else {
        args.format
    };
    let output = args.output.clone().unwrap_or_else(|| {
        std::path::PathBuf::from(format!(
            "mock-{}x{}.{}",
            args.width,
            args.height,
            format.extension()
        ))
    });
    if output.as_os_str() == "-" {
        return Err(anyhow!(
            "stdout output is not supported for mock in v0.1; pass an explicit -o path"
        ));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    img.save(&output)?;
    log_written(Some(&output), 0, format, args.width, args.height);
    Ok(())
}

fn run_watermark(args: WatermarkArgs) -> Result<()> {
    let img = decode_input(&args.io.input)?;
    let content = if let Some(text) = &args.text {
        WatermarkContent::Text { text: text.clone() }
    } else if let Some(path) = &args.image {
        let wm = decode_input(path)?;
        WatermarkContent::Image(wm)
    } else {
        return Err(anyhow!(
            "internal: clap should have required --text or --image"
        ));
    };
    let font = match &args.font {
        Some(path) => Font::from_path(path)
            .with_context(|| format!("failed to load font {}", path.display()))?,
        None => Font::default_font(),
    };
    let opts = WatermarkOpts {
        content,
        position: args.position,
        opacity: args.opacity,
        margin: args.margin,
        scale: args.scale,
        text_color: Color::WHITE,
        font,
    };
    let result = img.watermark(opts)?;
    write_image_output(&result, &args.io, "watermarked")
}

fn run_compress(args: CompressArgs) -> Result<()> {
    let inputs = &args.inputs;
    if inputs.is_empty() {
        return Err(anyhow!("compress requires at least one INPUT path"));
    }

    let output_mode = resolve_output_mode(&args.output, inputs.len())?;
    let quality = build_quality(&args)?;
    for input in inputs {
        let img = decode_input(input)?;
        let per_output = match &output_mode {
            OutputMode::SingleFile(path) => path.clone(),
            OutputMode::Directory(dir) => derive_into_dir(dir, input, args.format, "compressed"),
            OutputMode::Auto => derive_next_to_input(input, args.format, "compressed"),
            OutputMode::Stdout => PathBuf::from("-"),
        };
        let format = resolve_compress_format(args.format, &per_output)?;
        let opts = CompressOpts {
            format,
            quality,
            strip_metadata: args.strip_metadata,
            effort: args.effort,
        };
        let encoded = img.compress(opts)?;
        write_bytes_output(Some(&per_output), &encoded.bytes)?;
        log_written(
            Some(&per_output),
            encoded.bytes.len(),
            encoded.format,
            encoded.size.width,
            encoded.size.height,
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
enum OutputMode {
    SingleFile(PathBuf),
    Directory(PathBuf),
    Auto,
    Stdout,
}

fn resolve_output_mode(output: &Option<PathBuf>, input_count: usize) -> Result<OutputMode> {
    let Some(path) = output else {
        return Ok(OutputMode::Auto);
    };
    if path.as_os_str() == "-" {
        if input_count > 1 {
            return Err(anyhow!("stdout output (-) requires a single input"));
        }
        return Ok(OutputMode::Stdout);
    }
    // Heuristic: if path exists and is a dir, OR path ends with separator,
    // OR there are multiple inputs, treat as dir.
    let looks_like_dir = path.is_dir()
        || path.as_os_str().to_string_lossy().ends_with(std::path::MAIN_SEPARATOR)
        || input_count > 1;
    if looks_like_dir {
        fs::create_dir_all(path)
            .with_context(|| format!("failed to create directory {}", path.display()))?;
        return Ok(OutputMode::Directory(path.clone()));
    }
    Ok(OutputMode::SingleFile(path.clone()))
}

fn derive_into_dir(dir: &Path, input: &Path, format: Format, suffix: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    let ext = if format != Format::Auto {
        format.extension().to_string()
    } else {
        input
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "png".to_string())
    };
    dir.join(format!("{stem}.{suffix}.{ext}"))
}

fn derive_next_to_input(input: &Path, format: Format, suffix: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    let ext = if format != Format::Auto {
        format.extension().to_string()
    } else {
        input
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "png".to_string())
    };
    let parent = input.parent().unwrap_or(Path::new("."));
    parent.join(format!("{stem}.{suffix}.{ext}"))
}

fn resolve_compress_format(flag: Format, output_path: &Path) -> Result<Format> {
    if flag != Format::Auto {
        return Ok(flag);
    }
    if output_path.as_os_str() == "-" {
        return Err(anyhow!(
            "stdout output needs an explicit --format (no path extension to infer from)"
        ));
    }
    Format::from_path(output_path).ok_or_else(|| {
        anyhow!(
            "could not infer output format from {} — pass --format",
            output_path.display()
        )
    })
}

// ---------------- shared IO ----------------

fn decode_input(path: &Path) -> Result<Image> {
    let bytes = read_input(path)?;
    Image::decode(&bytes).with_context(|| format!("failed to decode {}", path.display()))
}

fn read_input(path: &Path) -> Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        let mut buf = Vec::new();
        io::stdin()
            .read_to_end(&mut buf)
            .context("failed to read stdin")?;
        return Ok(buf);
    }
    fs::read(path).with_context(|| format!("failed to read {}", path.display()))
}

/// Write an [`Image`] to disk. For non-compress ops we go through the `image`
/// crate's default encoders (picked from the output path extension), since
/// these ops don't expose a quality knob — chain with `nupic compress` if you
/// want format-aware compression.
fn write_image_output(img: &Image, io_args: &CommonIo, suffix: &str) -> Result<()> {
    let path = derive_output_path(io_args, suffix);
    let path = path.ok_or_else(|| anyhow!("output path must be specified"))?;
    if path.as_os_str() == "-" {
        return Err(anyhow!(
            "stdout output is not supported for resize/fit/circle in v0.1 \
             (no format hint); pass an explicit -o path"
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    img.save(&path)?;
    log_written(Some(&path), 0, format_from_path(&path), img.width(), img.height());
    Ok(())
}

fn write_bytes_output(path: Option<&Path>, bytes: &[u8]) -> Result<()> {
    if is_stdout(path) {
        io::stdout()
            .write_all(bytes)
            .context("failed to write stdout")?;
    } else {
        let p = path.expect("non-stdout path");
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        fs::write(p, bytes).with_context(|| format!("failed to write {}", p.display()))?;
    }
    Ok(())
}

fn is_stdout(path: Option<&Path>) -> bool {
    matches!(path, Some(p) if p.as_os_str() == "-")
}

/// Compute the output path. If `--output` is given, use it; otherwise derive
/// `<stem>.<suffix>.<ext>` next to the input.
fn derive_output_path(io_args: &CommonIo, suffix: &str) -> Option<PathBuf> {
    if let Some(out) = &io_args.output {
        return Some(out.clone());
    }
    let input = &io_args.input;
    if input.as_os_str() == "-" {
        return Some(PathBuf::from("-"));
    }
    let stem = input.file_stem()?.to_string_lossy().into_owned();
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let parent = input.parent().unwrap_or(Path::new("."));
    Some(parent.join(format!("{stem}.{suffix}.{ext}")))
}

fn build_quality(args: &CompressArgs) -> Result<Quality> {
    if args.lossless {
        return Ok(Quality::Lossless);
    }
    if let Some(dist) = args.target_dssim {
        return Ok(Quality::Perceptual(PerceptualTarget::Dssim(dist)));
    }
    if let Some(score) = args.target_ssim {
        return Ok(Quality::Perceptual(PerceptualTarget::Ssimulacra2(score)));
    }
    if let Some(dist) = args.target_butteraugli {
        return Ok(Quality::Perceptual(PerceptualTarget::Butteraugli(dist)));
    }
    if let Some(q) = args.quality {
        return Ok(Quality::Format(q));
    }
    Ok(Quality::Auto)
}

fn build_resize_mode(args: &ResizeArgs) -> Result<ResizeMode> {
    if let Some(s) = args.scale {
        return Ok(ResizeMode::Scale(s));
    }
    match (args.width, args.height) {
        (Some(w), Some(h)) => Ok(ResizeMode::Exact {
            width: w,
            height: h,
        }),
        (Some(w), None) => Ok(ResizeMode::Width(w)),
        (None, Some(h)) => Ok(ResizeMode::Height(h)),
        (None, None) => Err(anyhow!(
            "internal: clap should have required --width/--height/--scale"
        )),
    }
}

fn format_from_path(p: &Path) -> Format {
    Format::from_path(p).unwrap_or(Format::Auto)
}

fn log_written(path: Option<&Path>, bytes: usize, format: Format, w: u32, h: u32) {
    if is_stdout(path) {
        return;
    }
    let path_disp = path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    if bytes == 0 {
        eprintln!("wrote {format:?} {w}×{h} to {path_disp}");
    } else {
        eprintln!("wrote {bytes} bytes ({format:?}, {w}×{h}) to {path_disp}");
    }
}
