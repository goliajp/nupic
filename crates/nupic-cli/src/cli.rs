use clap::{Args, Parser, Subcommand};
use nupic_core::{DenoiseKind, Filter, FilterKind, FitMode, Format, Metric, Position};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "nupic",
    version,
    about = "nupic — nuclear picture handler. Multi-pipeline image toolkit.",
    long_about = "nupic is a cross-platform image processing CLI. Each subcommand \
                  performs one operation; today's implementations wrap mature crates, \
                  and are gradually replaced with self-built, zero-dep pipelines.",
    propagate_version = true,
    arg_required_else_help = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Increase log verbosity (repeatable: -v, -vv, -vvv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Resize an image to specified dimensions.
    Resize(ResizeArgs),
    /// Fit an image into a target box (contain / cover / fill / inside / outside).
    Fit(FitArgs),
    /// Mask an image into a circle.
    Circle(CircleArgs),
    /// Generate a placeholder mockup image with a dimension label.
    Mock(MockArgs),
    /// Overlay a text or image watermark.
    Watermark(WatermarkArgs),
    /// Re-encode an image with format-aware compression.
    Compress(CompressArgs),
    /// Compare two images with a perceptual / structural metric.
    Compare(CompareArgs),
    /// Crop to a rectangle (top-left x,y + size).
    Crop(CropArgs),
    /// Pixel-space filter (blur, sharpen, grayscale, hue, …).
    Filter(FilterArgs),
    /// Denoise: gaussian blur or per-channel median filter.
    Denoise(DenoiseArgs),
    /// Find the tightest bbox around the input's non-transparent pixels.
    /// Prints `x y width height` to stdout.
    Bbox(BboxArgs),
    /// Print shell completion script to stdout (eval / save it to your fpath).
    Completions(CompletionsArgs),
    /// Sweep a dataset directory across formats; report size / encode time /
    /// DSSIM. The cement-layer baseline against which future self-built
    /// codec stones are measured.
    Bench(BenchArgs),
}

#[derive(Debug, Args)]
pub struct BenchArgs {
    /// Directory containing images to benchmark.
    #[arg(value_name = "DATASET")]
    pub dataset: PathBuf,

    /// Comma-separated formats to test. Default: png,jpeg,webp,avif.
    #[arg(short = 'f', long, default_value = "png,jpeg,webp,avif")]
    pub formats: String,

    /// Maximum number of images to test (after filtering).
    #[arg(long, default_value_t = 100)]
    pub limit: usize,

    /// Encoder effort (compress's `--effort`). 0 (fastest) ..= 10 (slowest).
    /// Default matches `nupic compress` so bench numbers reflect what users
    /// actually get.
    #[arg(long, default_value_t = 5)]
    pub effort: u8,

    /// Compare each PNG output against a pinned external baseline (e.g.
    /// `assets/png-bench/baseline.json` for the TinyPNG reference). When
    /// supplied, the run forces `--formats=png` and prints `tinypng_bytes`
    /// + `nupic/tinypng` columns. The process exits non-zero if any input
    /// file exceeds `1.15x` the baseline byte size.
    #[arg(long, value_name = "BASELINE_JSON")]
    pub baseline: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to emit completions for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[derive(Debug, Args)]
pub struct CommonIo {
    /// Input image path. Use '-' for stdin.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Output image path. Use '-' for stdout. Defaults to a derived name.
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Force output format. Default: inferred from output path extension.
    #[arg(short = 'f', long, value_enum, default_value_t = Format::Auto)]
    pub format: Format,
}

#[derive(Debug, Args)]
#[group(id = "resize_target", required = true, multiple = true)]
pub struct ResizeArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Target width in pixels.
    #[arg(short = 'W', long, group = "resize_target", conflicts_with = "scale")]
    pub width: Option<u32>,

    /// Target height in pixels.
    #[arg(short = 'H', long, group = "resize_target", conflicts_with = "scale")]
    pub height: Option<u32>,

    /// Scale factor (preserves aspect ratio). Mutually exclusive with -W / -H.
    #[arg(long, group = "resize_target")]
    pub scale: Option<f32>,

    /// Resampling filter.
    #[arg(long, value_enum, default_value_t = Filter::Lanczos3)]
    pub filter: Filter,
}

#[derive(Debug, Args)]
pub struct FitArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Target box width in pixels.
    #[arg(short = 'W', long)]
    pub width: u32,

    /// Target box height in pixels.
    #[arg(short = 'H', long)]
    pub height: u32,

    /// How the image is positioned inside the target box.
    #[arg(short = 'm', long, value_enum, default_value_t = FitMode::Contain)]
    pub mode: FitMode,

    /// Resampling filter.
    #[arg(long, value_enum, default_value_t = Filter::Lanczos3)]
    pub filter: Filter,

    /// Background color for padding (contain mode only).
    /// Accepts `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
    /// or `black` / `white` / `transparent`.
    #[arg(long, default_value = "transparent")]
    pub bg: String,
}

#[derive(Debug, Args)]
pub struct CircleArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Circle radius in pixels. Default: inscribed circle of the input image.
    #[arg(long)]
    pub radius: Option<u32>,

    /// Anti-aliasing feather width at the edge in pixels.
    #[arg(long, default_value_t = 1)]
    pub feather: u32,
}

#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub enum MockStyleArg {
    Stripes,
    Solid,
    Gradient,
    Checker,
}

#[derive(Debug, Args)]
pub struct MockArgs {
    /// Output image path. Use '-' for stdout.
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Output format. Default: png.
    #[arg(short = 'f', long, value_enum, default_value_t = Format::Png)]
    pub format: Format,

    /// Image width in pixels.
    #[arg(short = 'W', long, default_value_t = 800)]
    pub width: u32,

    /// Image height in pixels.
    #[arg(short = 'H', long, default_value_t = 600)]
    pub height: u32,

    /// Placeholder style.
    #[arg(long, value_enum, default_value_t = MockStyleArg::Stripes)]
    pub style: MockStyleArg,

    /// Checker tile size in pixels (only with `--style checker`).
    #[arg(long, default_value_t = 32)]
    pub tile: u32,

    /// Background color. Accepts `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa`
    /// or `black` / `white` / `transparent`.
    #[arg(long, default_value = "#e5e7eb")]
    pub bg: String,

    /// Foreground (label) color.
    #[arg(long, default_value = "#374151")]
    pub fg: String,

    /// Custom label text. Default: "<W> × <H>".
    #[arg(long)]
    pub text: Option<String>,

    /// Override the bundled font with a TTF / OTF file. Useful for CJK
    /// labels (e.g. `--font /System/Library/Fonts/PingFang.ttc`).
    #[arg(long, value_name = "PATH")]
    pub font: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct WatermarkArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Watermark text. Mutually exclusive with `--image`.
    #[arg(long, conflicts_with = "image", required_unless_present = "image")]
    pub text: Option<String>,

    /// Watermark image path. Mutually exclusive with `--text`.
    #[arg(long, conflicts_with = "text", required_unless_present = "text")]
    pub image: Option<PathBuf>,

    /// Anchor position.
    #[arg(short = 'p', long, value_enum, default_value_t = Position::BottomRight)]
    pub position: Position,

    /// Opacity, 0.0 (invisible) to 1.0 (opaque).
    #[arg(long, default_value_t = 0.5)]
    pub opacity: f32,

    /// Margin from the anchor edge in pixels.
    #[arg(long, default_value_t = 16)]
    pub margin: u32,

    /// Image-watermark scale, 0.0–1.0 of the base image width.
    #[arg(long, default_value_t = 0.2)]
    pub scale: f32,

    /// Override the bundled font for `--text` watermarks (TTF / OTF).
    #[arg(long, value_name = "PATH")]
    pub font: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct CompressArgs {
    /// Input image paths (one or more). Use `-` for stdin (single input only).
    /// With multiple inputs, `--output` must be a directory (created if needed),
    /// or omitted (each compressed file is written next to its source).
    #[arg(value_name = "INPUT", required = true, num_args = 1..)]
    pub inputs: Vec<PathBuf>,

    /// Output path. With one input: file or `-` for stdout. With multiple inputs:
    /// must be a directory.
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Force output format. Default: inferred from output path extension.
    #[arg(short = 'f', long, value_enum, default_value_t = Format::Auto)]
    pub format: Format,

    /// Format-native quality (0–100). Lossy formats only.
    /// Without this flag the encoder picks the visually-lossless default
    /// per format (Lossless for PNG/WebP/GIF/BMP/TIFF, 95 for JPEG, 90 for AVIF).
    #[arg(
        short = 'q',
        long,
        conflicts_with_all = ["lossless", "target_dssim", "target_ssim", "target_butteraugli"],
    )]
    pub quality: Option<u8>,

    /// Force lossless encoding (PNG / WebP-lossless / AVIF-lossless / JXL-lossless).
    #[arg(long, conflicts_with_all = ["target_dssim", "target_ssim", "target_butteraugli"])]
    pub lossless: bool,

    /// Target DSSIM distance (lower = better; 0.0 = identical;
    /// 0.005 ≈ visually lossless). Encoder binary-searches the smallest
    /// output that meets this. (Working — v0.3+.)
    #[arg(long, value_name = "DIST", conflicts_with_all = ["target_ssim", "target_butteraugli"])]
    pub target_dssim: Option<f32>,

    /// Target SSIMULACRA2 score (higher = better; typical 70–95). The encoder
    /// searches for the smallest output that meets this score.
    /// (Reserved — `NotImplemented` until the stone-layer SSIMULACRA2 lands.)
    #[arg(long, value_name = "SCORE", conflicts_with = "target_butteraugli")]
    pub target_ssim: Option<f32>,

    /// Target Butteraugli max-distance (lower = better; typical 0.5–3.0). The
    /// encoder searches for the smallest output that meets this distance.
    /// (Reserved — `NotImplemented` until the stone-layer Butteraugli lands.)
    #[arg(long, value_name = "DIST")]
    pub target_butteraugli: Option<f32>,

    /// Strip metadata (EXIF / XMP / ICC).
    #[arg(long)]
    pub strip_metadata: bool,

    /// Encoder effort, 0 (fastest) to 10 (slowest, best compression).
    #[arg(long, default_value_t = 5)]
    pub effort: u8,
}

#[derive(Debug, Args)]
pub struct CropArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Top-left X coordinate (in input image pixels). Default 0.
    #[arg(short = 'x', long, default_value_t = 0)]
    pub x: i32,

    /// Top-left Y coordinate. Default 0.
    #[arg(short = 'y', long, default_value_t = 0)]
    pub y: i32,

    /// Crop width in pixels.
    #[arg(short = 'W', long)]
    pub width: u32,

    /// Crop height in pixels.
    #[arg(short = 'H', long)]
    pub height: u32,
}

#[derive(Debug, Args)]
pub struct FilterArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Filter to apply.
    #[arg(short = 'k', long, value_enum)]
    pub kind: FilterKind,

    /// Filter strength / parameter. Semantics depend on `--kind`:
    /// blur / sharpen → sigma (px); brightness → [-255..=255];
    /// contrast → percent; hue → degrees. Default = per-kind sensible value.
    #[arg(short = 'a', long)]
    pub amount: Option<f32>,
}

#[derive(Debug, Args)]
pub struct DenoiseArgs {
    #[command(flatten)]
    pub io: CommonIo,

    /// Denoise mode.
    #[arg(short = 'k', long, value_enum, default_value_t = DenoiseKind::Median)]
    pub kind: DenoiseKind,

    /// Strength: gaussian = sigma in px; median = window radius (1 → 3×3 …).
    #[arg(short = 's', long, default_value_t = 1.0)]
    pub strength: f32,
}

#[derive(Debug, Args)]
pub struct BboxArgs {
    /// Input image path.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Alpha threshold (0..=255). Pixels with alpha strictly greater
    /// count as content. Default 0.
    #[arg(long, default_value_t = 0)]
    pub threshold: u8,
}

#[derive(Debug, Args)]
pub struct CompareArgs {
    /// Reference image.
    #[arg(value_name = "REFERENCE")]
    pub reference: PathBuf,

    /// Distorted image to compare against the reference.
    #[arg(value_name = "DISTORTED")]
    pub distorted: PathBuf,

    /// Metric to compute.
    #[arg(short = 'm', long, value_enum, default_value_t = Metric::Dssim)]
    pub metric: Metric,
}
