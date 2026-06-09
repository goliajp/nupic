use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use nupic_core::{
    CircleOpts, Color, CompressOpts, FitOpts, Font, Format, Image, MockOpts, MockStyle,
    PerceptualTarget, Quality, ResizeMode, ResizeOpts, Size, WatermarkContent, WatermarkOpts,
};

use crate::cli::{
    Cli, CircleArgs, Command, CommonIo, CompressArgs, FitArgs, MockArgs, MockStyleArg,
    ResizeArgs, WatermarkArgs,
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
    }
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
    let img = decode_input(&args.io.input)?;
    let output_path = derive_output_path(&args.io, "compressed");
    let format = resolve_format(&args.io, output_path.as_deref())?;
    let quality = build_quality(&args)?;
    let opts = CompressOpts {
        format,
        quality,
        strip_metadata: args.strip_metadata,
        effort: args.effort,
    };
    let encoded = img.compress(opts)?;
    write_bytes_output(output_path.as_deref(), &encoded.bytes)?;
    log_written(
        output_path.as_deref(),
        encoded.bytes.len(),
        encoded.format,
        encoded.size.width,
        encoded.size.height,
    );
    Ok(())
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

/// Resolve the output format from CLI flag + output path. Never returns
/// [`Format::Auto`].
fn resolve_format(io_args: &CommonIo, output_path: Option<&Path>) -> Result<Format> {
    if io_args.format != Format::Auto {
        return Ok(io_args.format);
    }
    if let Some(p) = output_path {
        if p.as_os_str() != "-" {
            if let Some(f) = Format::from_path(p) {
                return Ok(f);
            }
        }
    }
    Err(anyhow!(
        "could not infer output format — pass --format or use an output path with a known extension"
    ))
}

fn build_quality(args: &CompressArgs) -> Result<Quality> {
    if args.lossless {
        return Ok(Quality::Lossless);
    }
    if let Some(score) = args.target_ssim {
        return Ok(Quality::Perceptual(PerceptualTarget::Ssimulacra2(score)));
    }
    if let Some(dist) = args.target_butteraugli {
        return Ok(Quality::Perceptual(PerceptualTarget::Butteraugli(dist)));
    }
    Ok(Quality::Format(args.quality))
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
